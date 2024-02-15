use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc::{channel, Sender},
};

use embedded_web::{Error, HttpMethod, SocketAddr, WriteResult};

fn main() {
    let mut server: embedded_web::Server<_, 4> = embedded_web::Server::new(ServerImpl::default());

    let (sender, receiver) = channel::<ToMainLoop>();

    spawn_listener(sender.clone());
    while let Ok(msg) = receiver.recv() {
        match msg {
            ToMainLoop::NewClient(stream, addr) => {
                if let Err(e) = server.new_connection(addr) {
                    println!("Could not accept connection from {addr:?}: {e:?}");
                    continue;
                }
                println!("New connection from {addr:?}");
                spawn_client(
                    stream.try_clone().expect("Could not clone TCP stream"),
                    addr,
                    sender.clone(),
                );
                server.inner.clients.insert(addr, stream);
            }
            ToMainLoop::ClientDisconnected(addr) => {
                println!("Client {addr:?} disconnected");
                server.client_disconnected(addr);
                server.inner.clients.remove(&addr);
            }
            ToMainLoop::DataReceived(addr, data) => {
                if let Err(e) = server.data_received(addr, data.as_slice()) {
                    eprintln!("Could not process incoming data for client {addr:?}: {e:?}");
                    server.client_disconnected(addr);
                    server.inner.clients.remove(&addr);
                    continue;
                }
                if let Err(e) = server.poll_write() {
                    eprintln!("Could not poll write after data received: {e:?}");
                }
            }
        }
    }
}

fn spawn_client(mut stream: TcpStream, addr: SocketAddr, sender: Sender<ToMainLoop>) {
    std::thread::spawn(move || loop {
        let mut buffer = [0u8; 1024];
        let len = match stream.read(&mut buffer) {
            Ok(0) => {
                // reading 0 bytes indicates that the stream is closed, either by them or by us
                let _ = sender.send(ToMainLoop::ClientDisconnected(addr));
                return;
            }
            Ok(len) => len,
            Err(e) => {
                eprintln!("Could not read from client {addr:?}: {e:?}");
                let _ = sender.send(ToMainLoop::ClientDisconnected(addr));
                return;
            }
        };
        let _ = sender.send(ToMainLoop::DataReceived(
            addr,
            buffer.iter().take(len).copied().collect(),
        ));
    });
}

fn spawn_listener(sender: Sender<ToMainLoop>) {
    let listener = TcpListener::bind("localhost:8080").expect("Could not bind to port 8080");
    println!("Listening on http://localhost:8080/");
    std::thread::spawn(move || {
        while let Ok((connection, addr)) = listener.accept() {
            sender
                .send(ToMainLoop::NewClient(connection, addr))
                .unwrap();
        }
    });
}

enum ToMainLoop {
    NewClient(std::net::TcpStream, embedded_web::SocketAddr),
    ClientDisconnected(SocketAddr),
    DataReceived(SocketAddr, Vec<u8>),
}

#[derive(Default)]
struct ServerImpl {
    clients: HashMap<SocketAddr, TcpStream>,
}

impl embedded_web::ServerConfig for ServerImpl {
    type ConnectionType = ConnectionImpl;

    fn new_connection(
        &self,
        addr: embedded_web::SocketAddr,
        method: embedded_web::HttpMethod,
        path: &str,
    ) -> embedded_web::Result<Self::ConnectionType> {
        Ok(ConnectionImpl::new(addr, method, path))
    }

    fn on_client_done_writing(
        &mut self,
        addr: std::net::SocketAddr,
        _client: &mut Self::ConnectionType,
    ) {
        if let Err(e) = self.clients[&addr].shutdown(std::net::Shutdown::Both) {
            eprintln!("Could not shut down stream for {addr:?}: {e:?}");
        }
    }
    fn write(&mut self, addr: embedded_web::SocketAddr, bytes: &[u8]) -> embedded_web::WriteResult {
        if let Some(client) = self.clients.get_mut(&addr) {
            let res = client.write_all(bytes);
            if let Err(e) = res {
                eprintln!("Could not write data to client {addr:?}: {e:?}");
                WriteResult::Err(Error::InvalidClientState)
            } else {
                WriteResult::Ok {
                    bytes_written: bytes.len(),
                }
            }
        } else {
            eprintln!("Client {addr:?} was not found for writing");
            WriteResult::Err(Error::ClientNotFound)
        }
    }
}

struct ConnectionImpl {
    addr: SocketAddr,
    method: HttpMethod,
    path: String,
    headers: Vec<(String, String)>,
}

impl ConnectionImpl {
    fn new(addr: SocketAddr, method: HttpMethod, path: &str) -> ConnectionImpl {
        Self {
            addr,
            method,
            path: path.to_owned(),
            headers: Vec::new(),
        }
    }
}

impl embedded_web::Connection for ConnectionImpl {
    fn header_received(&mut self, key: &str, value: &str) -> embedded_web::ReadResult {
        self.headers.push((key.to_owned(), value.to_owned()));
        embedded_web::ReadResult::Ok
    }

    fn switch_from_headers_to_body(&mut self) -> embedded_web::ReadResult {
        if self.method == HttpMethod::Get {
            embedded_web::ReadResult::EndReading
        } else {
            embedded_web::ReadResult::Ok
        }
    }

    fn body_received(&mut self, bytes: &[u8]) -> embedded_web::ReadResult {
        println!("{:?} has received data {bytes:?}", self.addr);
        embedded_web::ReadResult::Ok
    }

    fn poll_write(
        &mut self,
        writer: &mut dyn embedded_web::ResponseWriter,
    ) -> embedded_web::WriteResult {
        writer.write_status(200, "OK")?;
        writer.write_response_header("content-type", "text/plain")?;
        writer.end_headers_start_body()?;
        writeln!(writer, "Hello world from embedded web server")?;
        writeln!(writer)?;
        writeln!(writer, "Method: {:?}", self.method)?;
        writeln!(writer, "Requested path: {:?}", self.path)?;
        writeln!(writer)?;
        writeln!(writer, "Received headers:")?;
        for (k, v) in &self.headers {
            writeln!(writer, "    {k} = {v}")?;
        }
        embedded_web::WriteResult::EndWriting
    }
}
