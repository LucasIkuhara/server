use ws;
// use std::sync::mpsc::{Receiver, Sender, channel};
use std::env::var;
use std::process::exit;
use std::sync::mpsc::{Sender, Receiver, channel};
use std::sync::{Mutex, Arc};
use std::thread;
use std::io::{stdin, stdout, Write};
use std::collections::HashMap;


/// All possible message types in the system.
#[derive(Debug)]
enum MessageType {
    Request,
    Grant,
    Release
}

/// Represents a request received from a client.
struct Request {
    remote_process: String,
    callback_sender: Sender<String>,
    message_type: MessageType
}

impl Request {
    
    /// Create a new Request from a message and it's respective sender for responding to its creator.
    fn from_message(input: &ws::Message, socket_id: u32) -> (Request, Receiver<String>) {

        let text: &str = input.as_text().expect("Failed to parse received message.");
        let values: Vec<&str> = text.split("|").collect();
        let (tx, rx): (Sender<String>, Receiver<String>) = channel();

        let req = Request {
            remote_process: values[0].to_string(),
            callback_sender: tx,
            message_type: if values[1] == "REQ" 
                {MessageType::Request} else {MessageType::Release}
        };

        return (req, rx);
    }
}

/// Implement Display to make Request printable
impl std::fmt::Display for Request {

    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "\t- [from: {}, type: {:?}]", self.remote_process, self.message_type)
    }
}

struct QueueState {
    current_holder: Option<String>,
    queue: Vec<Request>,
    statistics: HashMap<String, i32>
}

/// Compute the server URL based on the HOST and PORT environment variables.
/// If HOST is unavailable, default to "0.0.0.0",
/// and if PORT is unavailable, default to 5000.
/// The resulting URL will not have the ws:// prefix.
fn build_url() -> String {

    let host: String = match var("HOST") {
        Ok(val) => val,
        Err(_) => String::from("0.0.0.0")
    };

    let port: String = match var("PORT") {
        Ok(val) => val,
        Err(_) => String::from("5050")
    };

    return format!("{}:{}", host, port);
}

/// Display all possible CLI commands.
fn cli_help() {
    println!("Available commands:\n\t0. Display this message.\n\t1. Show current queue\n\t2. Metrics by client\n\t3. Terminate execution\n");
}

/// Display a command-line menu to the user, and handle input appropriately.
fn handle_cli_input(queue: Arc<Mutex<QueueState>>) {

    cli_help();
    loop {

        // Read command
        print!(">> ");
        stdout().flush().unwrap();
        let mut buffer: String = String::new();
        stdin().read_line(&mut buffer).expect("Failed to read command.");
        let command = buffer.trim();

        // Handle input
        match command {

            "0" => cli_help(),
            "1" => {
                let mutex_q:std::sync::MutexGuard<'_, QueueState> = queue.lock().unwrap();

                println!("(HEAD)");
                for req in mutex_q.queue.iter() {
                    println!("{}", req);
                }
            },
            "2" => println!("Not implemented"),
            "3" => exit(0),
            _ => println!("Invalid command. To list available commands, type '0'.")
        }
    }

}

/// Main coordinator thread-target function. Responsible for managing access.
fn handle_queue(queue: Arc<Mutex<QueueState>>, rx: Receiver<Request>) {
    loop {
            // Wait for requests
            let data = rx.recv().expect("Coordinator failed to receive value from the request handler closure.");
    
            // Acquire state lock
            let mut state = queue.lock().unwrap();

            // Update statistics
            let current_stat = match state.statistics.get(&data.remote_process) {
                Some(val) => *val + 1,
                None => 1
            };
            state.statistics.insert(data.remote_process.clone(), current_stat);

            // Either execute, ignore or put the request in queue
            match state.current_holder {                
                Some(holder) => {
                    
                    data.callback_sender.send(String::from("This is a response")).unwrap();
                    state.queue.push(data);
                },

                // If there is no one accessing the critical zone
                None => {
                    match data.message_type {

                        // If message is a request, allow it access
                        MessageType::Request => {
                            print!("");
                        },

                        // If it a release, ignore it
                        _ => continue
                    }
                }
            }
    }
}

/// Handle user requests.
fn handle_request(client: ws::Sender, queue_sender: Arc<Sender<Request>>) -> impl Fn(ws::Message) -> ws::Result<()> {

    move |msg: ws::Message|  {

        // Create request struct from contents
        let (req, rx) = Request::from_message(&msg, 12);
        
        // Send Request to request queue, and wait for a response
        queue_sender.send(req).unwrap();
        let response = rx.recv().expect("Request closure failed to receive an answer.");

        // Respond to client upon receiving coordinator response
        client.send(response).unwrap();
        Ok(())
   }
}


fn main() {

    // Request queue and state initialization
    let mut state: QueueState = QueueState {
        current_holder: None,
        queue: vec![],
        statistics: HashMap::new()
    };
    let state_lock: Arc<Mutex<QueueState>> = Arc::new(Mutex::new(state));

    // Get server url
    let address = build_url();
    println!("Starting WS server at ws://{}", address);

    // Start CLI in another thread
    let cli_queue = state_lock.clone();
    thread::spawn(move || handle_cli_input(cli_queue));

    // Start coordinator in another thread
    let (tx, rx): (Sender<Request>, Receiver<Request>) = channel();
    thread::spawn(move || handle_queue(state_lock, rx));

    // Create a moveable copy of tx
    let transmitter = Arc::new(tx);

    // Create WS server    
    ws::listen(
        address, 
        |out| handle_request(out, transmitter.clone())
    ).expect("Failed to create WS server. Aborting.");
}
