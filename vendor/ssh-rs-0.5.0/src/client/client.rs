use crate::{
    algorithm::compression::{CompressNone, Compression},
    config::algorithm::AlgList,
};
use crate::{algorithm::encryption::Encryption, config::Config};
use crate::{algorithm::encryption::EncryptionNone, model::Sequence};
use std::time::Duration;

// the underlay connection
pub(crate) struct Client {
    pub(super) sequence: Sequence,
    pub(super) config: Config,
    pub(super) negotiated: AlgList,
    pub(super) encryptor: Box<dyn Encryption>,
    pub(super) compressor: Box<dyn Compression>,
    pub(super) session_id: Vec<u8>,
    /// ssh4 local modification: bytes of the in-flight inbound packet.
    /// Reads that stop short (nonblocking stream, read timeout) park their
    /// partial packet here and resume on the next call, instead of dropping
    /// bytes and desynchronizing the transport.
    pub(crate) recv_buf: Vec<u8>,
}

impl Client {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            encryptor: Box::<EncryptionNone>::default(),
            compressor: Box::<CompressNone>::default(),
            negotiated: AlgList::new(),
            session_id: vec![],
            sequence: Sequence::new(),
            recv_buf: vec![],
        }
    }

    pub fn get_encryptor(&mut self) -> &mut dyn Encryption {
        self.encryptor.as_mut()
    }

    pub fn get_compressor(&mut self) -> &mut dyn Compression {
        self.compressor.as_mut()
    }

    pub fn get_seq(&mut self) -> &mut Sequence {
        &mut self.sequence
    }

    pub fn get_timeout(&self) -> Option<Duration> {
        self.config.timeout
    }

    pub fn set_timeout(&mut self, tm: Option<Duration>) {
        self.config.timeout = tm
    }
}
