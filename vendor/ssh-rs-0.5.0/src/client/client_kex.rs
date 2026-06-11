#[cfg(feature = "deprecated-zlib")]
use crate::algorithm::{compression, Compress};
use crate::{
    algorithm::{
        encryption,
        hash::{self, HashCtx},
        key_exchange::{self, KeyExchange},
        mac,
        public_key::{self, PublicKey},
        Digest,
    },
    client::Client,
    config::algorithm::AlgList,
    constant::ssh_transport_code,
    error::{SshError, SshResult},
    model::{Data, Packet, SecPacket},
};
use std::io::{Read, Write};
use tracing::*;

/// ssh4 local modification: minimum timeout for a (re-)key exchange. The
/// interactive read timeout (tens of milliseconds) is far shorter than the
/// network round trips a kex needs; running the kex under it would abort
/// halfway and corrupt the transport state.
const KEX_TIMEOUT_FLOOR: std::time::Duration = std::time::Duration::from_secs(30);

impl Client {
    /// ssh4 local modification: run the key exchange with the session timeout
    /// raised to at least [`KEX_TIMEOUT_FLOOR`], and surface a timeout as a
    /// fatal [`SshError::KexError`] — callers treat plain `TimeoutError` as a
    /// benign idle read, but a half-finished kex is unrecoverable.
    pub fn key_agreement<S>(
        &mut self,
        stream: &mut S,
        server_algs: AlgList,
        digest: &mut Digest,
    ) -> SshResult<()>
    where
        S: Read + Write,
    {
        let saved = self.config.timeout;
        self.config.timeout = match saved {
            Some(t) if t < KEX_TIMEOUT_FLOOR => Some(KEX_TIMEOUT_FLOOR),
            other => other,
        };
        let result = self.key_agreement_inner(stream, server_algs, digest);
        self.config.timeout = saved;
        result.map_err(|e| match e {
            SshError::TimeoutError => SshError::KexError("key exchange timed out".to_owned()),
            e => e,
        })
    }

    fn key_agreement_inner<S>(
        &mut self,
        stream: &mut S,
        server_algs: AlgList,
        digest: &mut Digest,
    ) -> SshResult<()>
    where
        S: Read + Write,
    {
        // initialize the hash context
        digest.hash_ctx.set_v_c(&self.config.ver.client_ver);
        digest.hash_ctx.set_v_s(&self.config.ver.server_ver);

        info!("start for key negotiation.");
        info!("send client algorithm list.");

        let algs = self.config.algs.clone();
        let client_algs = algs.pack(self);
        digest.hash_ctx.set_i_c(client_algs.get_inner());
        client_algs.write_stream(stream)?;

        let negotiated = self.config.algs.match_with(&server_algs)?;

        // key exchange algorithm
        let mut key_exchange = key_exchange::from(&negotiated.key_exchange[0])?;
        self.send_qc(stream, key_exchange.get_public_key())?;

        // host key algorithm
        let mut public_key = public_key::from(&negotiated.public_key[0]);

        // generate session id
        let session_id = {
            let session_id = self.verify_signature_and_new_keys(
                stream,
                &mut public_key,
                &mut key_exchange,
                &mut digest.hash_ctx,
            )?;

            if self.session_id.is_empty() {
                session_id
            } else {
                self.session_id.clone()
            }
        };

        let hash = hash::Hash::new(
            digest.hash_ctx.clone(),
            &session_id,
            key_exchange.get_hash_type(),
        );

        // mac algorithm
        let mac = mac::from(&negotiated.c_mac[0]);

        // encryption algorithm
        let encryption = encryption::from(&negotiated.c_encryption[0], hash, mac);

        self.session_id = session_id;
        self.negotiated = negotiated;
        self.encryptor = encryption;

        #[cfg(feature = "deprecated-zlib")]
        {
            if let Compress::Zlib = self.negotiated.c_compress[0] {
                let comp = compression::from(&Compress::Zlib);
                self.compressor = comp;
            }
        }

        digest.key_exchange = Some(key_exchange);

        info!("key negotiation successful.");

        Ok(())
    }

    /// Send the public key
    fn send_qc<S>(&mut self, stream: &mut S, public_key: &[u8]) -> SshResult<()>
    where
        S: Read + Write,
    {
        let mut data = Data::new();
        data.put_u8(ssh_transport_code::KEXDH_INIT)
            .put_u8s(public_key);
        data.pack(self).write_stream(stream)
    }

    fn verify_signature_and_new_keys<S>(
        &mut self,
        stream: &mut S,
        public_key: &mut Box<dyn PublicKey>,
        key_exchange: &mut Box<dyn KeyExchange>,
        h: &mut HashCtx,
    ) -> SshResult<Vec<u8>>
    where
        S: Read + Write,
    {
        let mut session_id = vec![];
        loop {
            let mut data = Data::unpack(SecPacket::from_stream(stream, self)?)?;
            let message_code = data.get_u8();
            match message_code {
                ssh_transport_code::KEXDH_REPLY => {
                    // Generate the session id, get the signature
                    let sig = self.generate_signature(data, h, key_exchange)?;
                    // verify the signature
                    session_id = hash::digest(&h.as_bytes(), key_exchange.get_hash_type());
                    let flag = public_key.verify_signature(&h.k_s, &session_id, &sig)?;
                    if !flag {
                        let err_msg = "signature verification failure.".to_owned();
                        error!(err_msg);
                        return Err(SshError::KexError(err_msg));
                    }
                    info!("signature verification success.");
                }
                ssh_transport_code::NEWKEYS => {
                    self.new_keys(stream)?;
                    return Ok(session_id);
                }
                // ssh4 local modification: servers may interleave benign
                // transport messages (SSH_MSG_IGNORE, SSH_MSG_DEBUG, ...)
                // with the kex; skip them instead of panicking.
                x => {
                    debug!("ignore message {} during key exchange", x);
                    continue;
                }
            }
        }
    }

    /// get the signature
    fn generate_signature(
        &mut self,
        mut data: Data,
        h: &mut HashCtx,
        key_exchange: &mut Box<dyn KeyExchange>,
    ) -> SshResult<Vec<u8>> {
        let ks = data.get_u8s();
        h.set_k_s(&ks);
        // TODO:
        //   No fingerprint verification
        let qs = data.get_u8s();
        h.set_e(key_exchange.get_public_key());
        h.set_f(&qs);
        let vec = key_exchange.get_shared_secret(qs)?;
        h.set_k(&vec);
        let h = data.get_u8s();
        let mut hd = Data::from(h);
        hd.get_u8s();
        let signature = hd.get_u8s();
        Ok(signature)
    }

    /// NEWKEYS indicates that kex is done
    fn new_keys<S>(&mut self, stream: &mut S) -> SshResult<()>
    where
        S: Write,
    {
        let mut data = Data::new();
        data.put_u8(ssh_transport_code::NEWKEYS);
        info!("send new keys");
        data.pack(self).write_stream(stream)
    }
}
