// mini_controller.js
// Place this in your Bitwig controller scripts folder to test outbound sockets.
// Attempts to open a TCP socket to localhost:9000 and send a handshake.

function init() {
    host.println('mini_controller: init');
    try {
        // Attempt to use Java networking if available in the scripting host.
        var Socket = null;
        try {
            // Nashorn-style
            Socket = Java.type && Java.type('java.net.Socket');
        } catch (e) {
            // Rhino-style
            try { Socket = Packages.java.net.Socket; } catch (e2) { Socket = null; }
        }

        if (!Socket) {
            host.println('mini_controller: Java socket classes unavailable in this environment');
            return;
        }

        var sock = new Socket('127.0.0.1', 9000);
        host.println('mini_controller: socket opened');
        var out = sock.getOutputStream();
        var data = new java.lang.String('hello-from-bitwig').getBytes('UTF-8');
        out.write(data);
        out.flush();
        sock.close();
        host.println('mini_controller: handshake sent and socket closed');
    } catch (err) {
        host.println('mini_controller: exception: ' + err);
    }
}

function exit() {}

// Bitwig calls init() on load; keep simple for testing.
init();
