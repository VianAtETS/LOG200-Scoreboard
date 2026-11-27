import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.net.Socket;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;

/**
 * A team's client, profiled. "main": reads the socket in its main thread, like the course template.
 * "reader": a dedicated thread reads the socket all the time, which the profiler cannot see through.
 */
public class FakeClient {
    static long spin;

    /** A recursive search, so that the profiler finds a recursion depth. */
    static void search(int depth, long until) {
        if (depth == 0 || System.nanoTime() > until) {
            for (int i = 0; i < 10_000; i++) spin += i;
            return;
        }
        search(depth - 1, until);
        search(depth - 1, until);
    }

    static String think() {
        long until = System.nanoTime() + 60_000_000L;
        while (System.nanoTime() < until) search(6, until);
        System.err.print("une ligne de l'agent sans fin, "); // the profiler's line must still stand alone
        return "A1-B1-C1";
    }

    public static void main(String[] args) throws Exception {
        try (Socket s = new Socket("127.0.0.1", Integer.parseInt(args[0]))) {
            BufferedReader in = new BufferedReader(new InputStreamReader(s.getInputStream()));
            PrintWriter out = new PrintWriter(s.getOutputStream(), true);
            if (args[1].equals("main")) {
                for (String line; (line = in.readLine()) != null && !line.equals("end"); ) out.println(think());
                return;
            }
            BlockingQueue<String> lines = new LinkedBlockingQueue<>();
            Thread reader = new Thread(() -> {
                try {
                    for (String line; (line = in.readLine()) != null; ) lines.put(line);
                } catch (Exception ignored) {
                }
                lines.add("end");
            });
            reader.setDaemon(true);
            reader.start();
            while (!lines.take().equals("end")) out.println(think());
        }
    }
}
