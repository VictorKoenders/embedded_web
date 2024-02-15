use crate::{Error, HttpMethod, Result};
use core::net::SocketAddr;

pub trait ServerConfig {
    type ConnectionType: Connection;

    fn new_connection(
        &self,
        addr: SocketAddr,
        method: HttpMethod,
        path: &str,
    ) -> Result<Self::ConnectionType>;

    fn on_client_done_writing(&mut self, addr: SocketAddr, client: &mut Self::ConnectionType);
    fn write(&mut self, addr: SocketAddr, bytes: &[u8]) -> WriteResult;
}

pub(crate) struct ServerConfigWriter<'a, S: ServerConfig> {
    pub(crate) addr: SocketAddr,
    pub(crate) config: &'a mut S,
}

impl<'a, S: ServerConfig> ResponseWriter for ServerConfigWriter<'a, S> {
    fn write_fmt(&mut self, fmt: core::fmt::Arguments) -> WriteResult {
        struct FmtWriter<'a, S: ServerConfig> {
            addr: SocketAddr,
            config: &'a mut S,
            response: Option<WriteResult>,
        }

        impl<'a, S: ServerConfig> core::fmt::Write for FmtWriter<'a, S> {
            fn write_str(&mut self, s: &str) -> core::fmt::Result {
                let res = self.config.write(self.addr, s.as_bytes());
                // only overwrite the response if the previous wasn't an error
                if !self.response.as_ref().map_or(false, |r| r.is_err()) {
                    self.response = Some(res);
                }

                // response here should always be `Some`
                if self.response.as_ref().unwrap().is_err() {
                    Err(core::fmt::Error)
                } else {
                    Ok(())
                }
            }
        }
        let mut writer = FmtWriter {
            addr: self.addr,
            config: self.config,
            response: None,
        };
        // we don't care about this error because we're going to use `response` instead
        let _ = core::fmt::write(&mut writer, fmt);
        writer
            .response
            .unwrap_or(WriteResult::Ok { bytes_written: 0 })
    }
}

pub trait Connection {
    fn header_received(&mut self, key: &str, value: &str) -> ReadResult;
    fn switch_from_headers_to_body(&mut self) -> ReadResult;
    fn body_received(&mut self, bytes: &[u8]) -> ReadResult;
    fn poll_write(&mut self, writer: &mut dyn ResponseWriter) -> WriteResult;
}

pub enum ReadResult {
    Ok,
    Err(Error),
    EndReading,
}

pub enum WriteResult {
    Ok { bytes_written: usize },
    Err(Error),
    EndWriting,
}

impl WriteResult {
    pub fn is_err(&self) -> bool {
        matches!(self, Self::Err(_))
    }
}

impl core::ops::FromResidual for WriteResult {
    fn from_residual(residual: Self) -> Self {
        residual
    }
}

pub trait ResponseWriter {
    fn write_fmt(&mut self, fmt: core::fmt::Arguments) -> WriteResult;

    fn write_status(&mut self, code: u16, value: &str) -> WriteResult {
        writeln!(self, "HTTP/1.1 {code} {value}")
    }

    fn write_response_header(&mut self, key: &str, value: &str) -> WriteResult {
        writeln!(self, "{key}: {value}")
    }
    fn end_headers_start_body(&mut self) -> WriteResult {
        writeln!(self)
    }
}

impl core::ops::Try for WriteResult {
    type Output = usize;
    type Residual = WriteResult;

    fn from_output(bytes_written: usize) -> Self {
        Self::Ok { bytes_written }
    }

    fn branch(self) -> core::ops::ControlFlow<Self::Residual, Self::Output> {
        match self {
            Self::Ok { bytes_written } => core::ops::ControlFlow::Continue(bytes_written),
            _ => core::ops::ControlFlow::Break(self),
        }
    }
}
