use tinyroute::block_on;
use tinyroute_websockets::server::run;

fn main() {
    block_on(run());
}


/*

function open(event) {
    ws.send("Florp");
}

function message(message) {
    console.log("Message: ", message.data);
}

let ws = new WebSocket("ws://127.0.0.1:7000");
ws.onopen = open;
ws.onmessage = message;

*/


