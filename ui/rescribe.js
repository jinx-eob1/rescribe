let host = window.location.hostname;

let socket = new WebSocket("ws:/" + host + ":7625/ws");

let post_to_rescribe = (text) => {
    const data = {
        language: "jp",
        translated_text: text,
        // We don't want our replayed message loop back here
        prevent_ws_forward: true
    }

    const xhr = new XMLHttpRequest();

    // Open a POST request to the specified IP and port
    xhr.open('POST', "http://" + host + ":7625/queue", true);
    xhr.setRequestHeader('Content-Type', 'application/json');
    xhr.onreadystatechange = function() {
      if (xhr.readyState === XMLHttpRequest.DONE) {
        if (xhr.status === 200) {
          console.log('Successful post');
        } else {
          console.error('Error:', xhr.statusText);
        }
      }
    };
    xhr.send(JSON.stringify(data));
}

socket.onopen = function(e) {
    console.log("[open] Connection established");
};

socket.onmessage = function(event) {
    console.log(`[message] Data received from server: ${event.data}`);

    // Translated text + button
    const button = document.createElement('button');
    button.innerText = "▶"
    document.body.appendChild(button);
    button.addEventListener('click', function() {
        post_to_rescribe(event.data)
        console.log(event.data);
    });
    document.body.append(" " + event.data);


    // Line break
    var br = document.createElement("br");
    document.body.appendChild(br);

};

socket.onclose = function(event) {
    if (event.wasClean) {
        console.log(`[close] Connection closed cleanly, code=${event.code} reason=${event.reason}`);
    } else {
        console.log('[close] Connection died');
    }
};

socket.onerror = function(error) {
    console.log(`[error]`);
};
