import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.file.Files;
import java.nio.file.Path;

/** Stands in for the referee, in its own JVM: MOVES times "go", then waits for the move; "end" at the end. READY is created once it listens. */
public class FakeServer {
    public static void main(String[] args) throws Exception {
        int port = Integer.parseInt(args[0]), moves = Integer.parseInt(args[1]);
        try (ServerSocket server = new ServerSocket(port)) {
            Files.createFile(Path.of(args[2]));
            play(server.accept(), moves);
        }
    }

    static void play(Socket client, int moves) throws Exception {
        try (Socket s = client) {
            BufferedReader in = new BufferedReader(new InputStreamReader(s.getInputStream()));
            PrintWriter out = new PrintWriter(s.getOutputStream(), true);
            for (int i = 0; i < moves; i++) {
                Thread.sleep(80); // the opponent's turn: the client waits in a read
                out.println("go");
                if (in.readLine() == null) return;
            }
            out.println("end");
        }
    }
}
