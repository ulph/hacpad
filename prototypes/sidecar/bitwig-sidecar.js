// bitwig-sidecar.js
// Minimal Node.js TCP server for testing Bitwig controller script socket outbound.

const net = require('net');
const PORT = 9000;

const server = net.createServer((socket) => {
  console.log('sidecar: connection from', socket.remoteAddress, socket.remotePort);
  socket.on('data', (data) => {
    console.log('sidecar: received:', data.toString());
    // simple response
    socket.write('ACK');
  });
  socket.on('end', () => console.log('sidecar: connection ended'));
});

server.listen(PORT, '127.0.0.1', () => {
  console.log(`sidecar: listening on 127.0.0.1:${PORT}`);
});

// Run with: node prototypes/sidecar/bitwig-sidecar.js
