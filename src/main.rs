fn main() {
}


/*

function open(event) {
    ws.send("hi|bye");
    let obj = {Subscribe: {level: null,modules:[]}}
    let j = JSON.stringify(obj);
    let payload = "log|" + j;
    ws.send(payload);
}

function message(message) {
    console.log("Message: ", message.data);
}

let ws = new WebSocket("ws://127.0.0.1:5567");
ws.onopen = open;
ws.onmessage = message;

*/


