use crate::http_utilities::HeaderLine;
use crate::traits::{Connection, ReadResult};
use crate::{Error, Result};
use core::net::SocketAddr;

pub(crate) struct Client<T: Connection> {
    pub(crate) connection: Option<T>,
    pub(crate) state: ClientState,
    pub(crate) address: SocketAddr,
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub(crate) enum ClientState {
    ReadingRequestLine,
    ReadingHeaders,
    ReadingBody,
    Writing,
    Done,
}

impl<T: Connection> Client<T> {
    pub(crate) fn new(address: SocketAddr) -> Self {
        Self {
            connection: None,
            state: ClientState::ReadingRequestLine,
            address,
        }
    }

    pub(crate) fn data_received<S>(&mut self, config: &mut S, mut data: &[u8]) -> Result
    where
        S: crate::ServerConfig<ConnectionType = T>,
    {
        loop {
            return match (self.state, &mut self.connection) {
                (ClientState::ReadingRequestLine, None) => {
                    let (method, path, remaining) =
                        crate::http_utilities::read_http_request_line(data)?;
                    let connection = config.new_connection(self.address, method, path)?;
                    self.connection = Some(connection);
                    self.state = ClientState::ReadingHeaders;
                    data = remaining;
                    continue;
                }
                (ClientState::ReadingHeaders, Some(connection)) => {
                    match crate::http_utilities::read_header_line(data) {
                        HeaderLine::Error(e) => Err(e),
                        HeaderLine::Whiteline { remaining } => {
                            match connection.switch_from_headers_to_body() {
                                ReadResult::Ok => {
                                    self.state = ClientState::ReadingBody;
                                    data = remaining;
                                    continue;
                                }
                                ReadResult::Err(e) => Err(e),
                                ReadResult::EndReading => {
                                    self.state = ClientState::Writing;
                                    Ok(())
                                }
                            }
                        }
                        HeaderLine::Header {
                            key,
                            value,
                            remaining,
                        } => match connection.header_received(key, value) {
                            ReadResult::Ok => {
                                data = remaining;
                                continue;
                            }
                            ReadResult::Err(e) => Err(e),
                            ReadResult::EndReading => {
                                self.state = ClientState::Writing;
                                Ok(())
                            }
                        },
                    }
                }
                (ClientState::ReadingBody, Some(connection)) => {
                    match connection.body_received(data) {
                        ReadResult::Ok => Ok(()),
                        ReadResult::Err(e) => Err(e),
                        ReadResult::EndReading => {
                            self.state = ClientState::Writing;
                            Ok(())
                        }
                    }
                }
                (ClientState::Writing | ClientState::Done, _) => {
                    // do nothing
                    Ok(())
                }
                _ => Err(Error::InvalidClientState),
            };
        }
    }

    pub(crate) fn poll_write<S: crate::ServerConfig<ConnectionType = T>>(
        &mut self,
        config: &mut S,
    ) -> crate::WriteResult {
        if let Some(connection) = &mut self.connection {
            connection.poll_write(&mut crate::traits::ServerConfigWriter {
                config,
                addr: self.address,
            })
        } else {
            crate::WriteResult::EndWriting
        }
    }
}
