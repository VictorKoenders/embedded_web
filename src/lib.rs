#![no_std]
// should be stable rust 1.77, march 21th 2024
#![feature(ip_in_core)]
// Useful tools for dealing with ascii characters
// This one is stable but the API might change
// tracking issue: https://github.com/rust-lang/rust/issues/110998
#![feature(ascii_char, ascii_char_variants)]
// Custom try implementation, it's neat
// Seems to be mostly stable
// tracking issue: https://github.com/rust-lang/rust/issues/84277
#![feature(try_trait_v2)]

mod client;
mod http_utilities;
mod traits;

use client::{Client, ClientState};
use heapless::Vec;

pub use core::net::SocketAddr;
pub use http_utilities::HttpMethod;
pub use traits::{Connection, ReadResult, ResponseWriter, ServerConfig, WriteResult};

pub struct Server<T: ServerConfig, const NUMBER_OF_CONNECTIONS: usize> {
    pub inner: T,
    clients: Vec<Client<T::ConnectionType>, NUMBER_OF_CONNECTIONS>,
}
impl<T: ServerConfig, const NUMBER_OF_CONNECTIONS: usize> Server<T, NUMBER_OF_CONNECTIONS> {
    pub fn new(config: T) -> Self {
        Self {
            inner: config,
            clients: Vec::new(),
        }
    }

    pub fn new_connection(&mut self, addr: SocketAddr) -> Result {
        if let Some(idx) = self.clients.iter().position(|c| c.address == addr) {
            if self.clients[idx].state != ClientState::Done {
                return Err(Error::AlreadyConnected);
            }
            self.clients.remove(idx);
        }
        if self.clients.push(Client::new(addr)).is_err() {
            Err(Error::TooManyConnections)
        } else {
            Ok(())
        }
    }

    pub fn client_disconnected(&mut self, addr: SocketAddr) {
        self.clients.retain(|c| c.address != addr);
    }

    pub fn data_received(&mut self, addr: SocketAddr, data: &[u8]) -> Result {
        if let Some(connection) = self.clients.iter_mut().find(|c| c.address == addr) {
            connection.data_received(&mut self.inner, data)
        } else {
            Err(Error::TooManyConnections)
        }
    }

    pub fn poll_write(&mut self) -> Result {
        for client in &mut self.clients {
            if client.state == ClientState::Writing {
                match client.poll_write(&mut self.inner) {
                    WriteResult::Ok { .. } => {}
                    WriteResult::Err(e) => {
                        return Err(e);
                    }
                    WriteResult::EndWriting => {
                        client.state = ClientState::Done;
                        if let Some(connection) = &mut client.connection {
                            self.inner
                                .on_client_done_writing(client.address, connection);
                        }
                    }
                }
            }
        }
        self.clients.retain(|c| c.state != ClientState::Done);
        Ok(())
    }
}

pub type Result<T = ()> = core::result::Result<T, Error>;

#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq, Eq))]
pub enum Error {
    TooManyConnections,
    InvalidRequestLine,
    InvalidHttpMethod,
    InvalidHttpHeader,
    AlreadyConnected,
    InvalidClientState,
    ClientNotFound,

    #[deprecated(note = "only used for development")]
    Unimplemented,
}
