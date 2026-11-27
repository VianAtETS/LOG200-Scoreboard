import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.PrintStream;
import java.lang.instrument.Instrumentation;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Sampling profiler of the ladder, loaded into a team's jar with -javaagent:/profiler.jar (the jar is not
 * modified). A variant of the profiler of the isola arena by Justin Pagès (gitlab.com/justin-pages/isola,
 * arena/src/Profiler.java), with his permission: same counting, same line format.
 *
 * The sandbox has no network, so no control socket tells the profiler when the agent's turn starts. It
 * infers it: the agent computes when none of its threads is blocked in a read (socket, console). Nothing
 * is counted before the first such read (JVM start, connection). A read shorter than PERIOD_MS can go
 * unseen: two turns then make one segment, when the opponent answers that fast. An agent whose dedicated
 * thread always reads the socket never computes for the profiler: its profile stays empty, and the ladder
 * marks it doubtful when the sampled time falls short of the agent's own thinking time.
 *
 * While the agent computes, every PERIOD_MS, it reads the stacks of every RUNNABLE platform thread running
 * agent code; virtual threads are not sampled. Per method of the agent:
 *   - self  : the method is the top of a stack (calls into the Java library are attributed to it);
 *   - total : the method is somewhere in the stack.
 * Recursion depth of a stack: the outermost method that appears twice opens the recursive cycle; the
 * cycle is every method met between its first and last occurrence; depth = frames of the cycle.
 *
 * One line on stderr at the end of each segment, and of the last one when the JVM exits, on a line of its own:
 *   prof n=<thread samples> t=<sampled ms> rec=<peak depth> cost=<sampling µs> end=1 Class.method=self:total ... @A.f>B.g=<samples> ...
 * "@path=n": n samples whose call path (recursion folded) is exactly that one. t: the ticks spent
 * computing, an estimate of the computing time. cost: time the profiler spent reading stacks while the
 * agent computed, taken from the agent's clock.
 */
public class Profiler {

    static final int PERIOD_MS = 10;

    static PrintStream out;

    /** The segment in progress; the sampler and the shutdown hook share it under the class lock. */
    static boolean inTurn = false;
    static int n = 0, rec = 0;
    static long sampledNs = 0, costNs = 0;
    /** Threads of the JVM, grown as needed. */
    static Thread[] threads = new Thread[64];

    public static void premain(String args, Instrumentation inst) {
        // Its own stream on fd 2: an agent that replaces System.err does not silence the profiler.
        out = new PrintStream(new FileOutputStream(FileDescriptor.err), true, StandardCharsets.UTF_8);
        Thread t = new Thread(Profiler::loop, "ladder-profiler");
        t.setDaemon(true);
        t.start();
        // The last segment: the agent often exits right after reading the end of the game.
        Runtime.getRuntime().addShutdownHook(new Thread(Profiler::end, "ladder-profiler-exit"));
    }

    static boolean jdk(String cls) {
        return cls.startsWith("java.") || cls.startsWith("jdk.") || cls.startsWith("sun.") || cls.startsWith("javax.") || cls.startsWith("com.sun.");
    }

    static boolean io(String cls) {
        return cls.startsWith("sun.nio.") || cls.startsWith("java.net.") || cls.startsWith("java.io.") || cls.startsWith("sun.net.");
    }

    static boolean agentCode(StackTraceElement[] st) {
        for (StackTraceElement f : st) if (!jdk(f.getClassName())) return true;
        return false;
    }

    /** A thread of the agent waiting for input: a native read or poll on top, agent code below. Writes do not count. */
    static boolean reading(StackTraceElement[] st) {
        if (st.length == 0 || !st[0].isNativeMethod() || !io(st[0].getClassName())) return false;
        String m = st[0].getMethodName();
        return (m.startsWith("read") || m.startsWith("poll") || m.equals("wait") || m.startsWith("accept")) && agentCode(st);
    }

    /** Counter of a method: {self, total, last sample seen, occurrences in that sample}. */
    static final Map<String, Map<String, int[]>> counters = new HashMap<>();
    static int sampleId = 0;

    static int[] counter(StackTraceElement f) {
        return counters.computeIfAbsent(f.getClassName(), k -> new HashMap<>()).computeIfAbsent(f.getMethodName(), k -> new int[4]);
    }

    /** Samples per call path (root > ... > top, agent frames only, recursion folded), for the call tree. */
    static final Map<String, int[]> paths = new HashMap<>();

    static String shortName(StackTraceElement f) {
        String c = f.getClassName();
        return c.substring(c.lastIndexOf('.') + 1) + '.' + f.getMethodName();
    }

    /**
     * Call path of a stack. Recursion is folded: when a method is already on the path, the path goes back to it,
     * so negamax > eval > negamax > eval is one node negamax with one child eval, whatever the depth.
     */
    static String path(StackTraceElement[] st) {
        List<String> p = new ArrayList<>();
        for (int i = st.length - 1; i >= 0; i--) {
            if (jdk(st[i].getClassName())) continue;
            String m = shortName(st[i]);
            int at = p.indexOf(m);
            if (at >= 0) p.subList(at + 1, p.size()).clear();
            else p.add(m);
        }
        return String.join(">", p);
    }

    /** Counts one stack of agent code; returns its recursion depth (0 if no method repeats). */
    static int sample(StackTraceElement[] st) {
        int id = ++sampleId;
        paths.computeIfAbsent(path(st), k -> new int[1])[0]++;
        int[][] c = new int[st.length][];
        for (int i = st.length - 1; i >= 0; i--) {
            if (jdk(st[i].getClassName())) continue;
            int[] k = c[i] = counter(st[i]);
            if (k[2] != id) {
                k[2] = id;
                k[3] = 0;
                k[1]++;
            }
            k[3]++;
        }
        for (int[] k : c) {
            if (k != null) {
                k[0]++;
                break;
            }
        }
        // From the root (end of the array) to the top: the outermost repeated method opens the cycle
        int open = -1, close = -1;
        for (int i = st.length - 1; i >= 0 && open < 0; i--) if (c[i] != null && c[i][3] >= 2) open = i;
        if (open < 0) return 0;
        for (int i = 0; i < open && close < 0; i++) if (c[i] == c[open]) close = i;
        // Cycle = methods met between the first and last occurrence of the head
        int mark = -id, depth = 0;
        for (int i = open; i >= close; i--) if (c[i] != null) c[i][2] = mark;
        for (int i = open; i >= 0; i--) if (c[i] != null && c[i][2] == mark) depth++;
        return depth;
    }

    static ThreadGroup root() {
        ThreadGroup g = Thread.currentThread().getThreadGroup();
        while (g.getParent() != null) g = g.getParent();
        return g;
    }

    static void loop() {
        Thread self = Thread.currentThread();
        ThreadGroup root = root();
        List<StackTraceElement[]> running = new ArrayList<>();
        boolean started = false;
        long last = System.nanoTime();

        while (true) {
            try {
                Thread.sleep(PERIOD_MS);
            } catch (InterruptedException e) {
                return;
            }
            synchronized (Profiler.class) {
                started = tick(self, root, running, started, last);
            }
            last = System.nanoTime();
        }
    }

    /** One tick of the sampler; returns whether the agent has started reading. */
    static boolean tick(Thread self, ThreadGroup root, List<StackTraceElement[]> running, boolean started, long last) {
        long now = System.nanoTime();
        int count;
        while ((count = root.enumerate(threads, true)) == threads.length) threads = new Thread[threads.length * 2];
        boolean waiting = false;
        running.clear();
        for (int i = 0; i < count; i++) {
            Thread t = threads[i];
            // A thread blocked in a native read is RUNNABLE too
            if (t == self || t.getState() != Thread.State.RUNNABLE) continue;
            StackTraceElement[] st = t.getStackTrace();
            if (reading(st)) waiting = true;
            else if (agentCode(st)) running.add(st);
        }
        started |= waiting;

        if (started && !waiting) {
            sampledNs += now - last;
            inTurn = true;
            for (StackTraceElement[] st : running) {
                n++;
                rec = Math.max(rec, sample(st));
            }
            costNs += System.nanoTime() - now;
        } else {
            end();
        }
        return started;
    }

    /** Closes the segment in progress, if any. */
    static synchronized void end() {
        if (!inTurn) return;
        emit();
        n = rec = 0;
        sampledNs = costNs = 0;
        inTurn = false;
    }

    static void emit() {
        // A line of its own, even after a partial line of the agent: the ladder keeps lines that start with "prof "
        StringBuilder sb = new StringBuilder("\nprof n=").append(n).append(" t=").append(sampledNs / 1_000_000)
                .append(" rec=").append(rec).append(" cost=").append(costNs / 1_000).append(" end=1");
        for (Map.Entry<String, Map<String, int[]>> e : counters.entrySet()) {
            String cls = e.getKey().substring(e.getKey().lastIndexOf('.') + 1);
            for (Map.Entry<String, int[]> m : e.getValue().entrySet()) {
                int[] k = m.getValue();
                if (k[0] == 0 && k[1] == 0) continue;
                sb.append(' ').append(cls).append('.').append(m.getKey()).append('=').append(k[0]).append(':').append(k[1]);
                k[0] = k[1] = 0;
            }
        }
        for (Map.Entry<String, int[]> e : paths.entrySet()) {
            if (e.getValue()[0] == 0) continue;
            sb.append(" @").append(e.getKey()).append('=').append(e.getValue()[0]);
            e.getValue()[0] = 0;
        }
        out.println(sb);
    }
}
