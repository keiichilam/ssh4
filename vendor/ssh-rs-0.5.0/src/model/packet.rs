use std::io::{Read, Write};
use std::time::Duration;

use crate::error::{SshError, SshResult};
use crate::{client::Client, model::Data};

use super::timeout::Timeout;

/// ## Binary Packet Protocol
///
/// <https://www.rfc-editor.org/rfc/rfc4253#section-6>
///
/// uint32 `packet_length`
///
/// byte `padding_length`
///
/// byte[[n1]] `payload`; n1 = packet_length - padding_length - 1
///
/// byte[[n2]] `random padding`; n2 = padding_length
///
/// byte[[m]] `mac` (Message Authentication Code - MAC); m = mac_length
///
/// ---
///
/// **packet_length**
/// The length of the packet in bytes, not including 'mac' or the 'packet_length' field itself.
///
///
/// **padding_length**
/// Length of 'random padding' (bytes).
///
///
/// **payload**
///  The useful contents of the packet.  If compression has been negotiated, this field is compressed.
/// Initially, compression MUST be "none".
///
///
/// **random padding**
/// Arbitrary-length padding, such that the total length of
/// (packet_length || padding_length || payload || random padding)
/// is a multiple of the cipher block size or 8, whichever is
/// larger.  There MUST be at least four bytes of padding.  The
/// padding SHOULD consist of random bytes.  The maximum amount of
/// padding is 255 bytes.

///
/// **mac**
/// Message Authentication Code.  If message authentication has
/// been negotiated, this field contains the MAC bytes.  Initially,
/// the MAC algorithm MUST be "none".。

/// ssh4 local modification: hard ceiling on the on-wire size of one packet.
/// A length beyond this means the stream is corrupt (or the peer is hostile);
/// failing fast beats allocating gigabytes from a garbage length field.
const MAX_PACKET_WIRE_LEN: usize = 1024 * 1024;

/// ssh4 local modification: append bytes from `stream` to `buf` until it holds
/// `want` bytes. Returns `Ok(false)` if the stream would block first — the
/// bytes read so far stay in `buf`, so the caller can resume later without
/// losing protocol framing. `Ok(0)` from the stream is a closed connection.
fn fill_buf<S>(stream: &mut S, buf: &mut Vec<u8>, want: usize) -> SshResult<bool>
where
    S: Read,
{
    let mut chunk = [0u8; 4096];
    while buf.len() < want {
        let need = (want - buf.len()).min(chunk.len());
        match stream.read(&mut chunk[..need]) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "connection closed by the remote host",
                )
                .into())
            }
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(true)
}

fn write_with_timeout<S>(stream: &mut S, tm: Option<Duration>, buf: &[u8]) -> SshResult<()>
where
    S: Write,
{
    let want_len = buf.len();
    let mut offset = 0;
    let mut timeout = Timeout::new(tm);

    loop {
        match stream.write(&buf[offset..]) {
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "connection closed by the remote host",
                )
                .into())
            }
            Ok(i) => {
                offset += i;
                if offset == want_len {
                    return Ok(());
                } else {
                    timeout.renew();
                    continue;
                }
            }
            Err(e) => {
                if let std::io::ErrorKind::WouldBlock = e.kind() {
                    timeout.till_next_tick()?;
                    continue;
                } else {
                    return Err(e.into());
                }
            }
        };
    }
}

pub(crate) trait Packet<'a> {
    fn pack(self, client: &'a mut Client) -> SecPacket<'a>;
    fn unpack(pkt: SecPacket) -> SshResult<Self>
    where
        Self: Sized;
}

pub(crate) struct SecPacket<'a> {
    payload: Data,
    client: &'a mut Client,
}

impl<'a> SecPacket<'a> {
    fn get_align(bsize: usize) -> i32 {
        let bsize = bsize as i32;
        if bsize > 8 {
            bsize
        } else {
            8
        }
    }

    pub fn write_stream<S>(self, stream: &mut S) -> SshResult<()>
    where
        S: Write,
    {
        let tm = self.client.get_timeout();
        let payload = self.client.get_compressor().compress(&self.payload)?;
        let payload_len = payload.len() as u32;
        let pad_len = {
            let mut pad = payload_len as i32 + 1;
            let block_size = Self::get_align(self.client.get_encryptor().bsize());
            if !self.client.get_encryptor().no_pad() {
                pad += 4
            }
            (((-pad) & (block_size - 1)) + block_size) as u32
        } as u8;
        let packet_len = 1 + pad_len as u32 + payload_len;
        let mut buf = vec![];
        buf.extend(packet_len.to_be_bytes());
        buf.extend([pad_len]);
        buf.extend(payload);
        buf.extend(vec![0; pad_len as usize]);
        let seq = self.client.get_seq().get_client();
        self.client.get_encryptor().encrypt(seq, &mut buf);
        write_with_timeout(stream, tm, &buf)
    }

    /// ssh4 local modification: assemble one packet out of the client's
    /// persistent receive buffer plus whatever the stream has available.
    /// Returns `Ok(None)` when the packet is still incomplete; the partial
    /// bytes stay buffered in the client, so a short read timeout can no
    /// longer desynchronize the transport mid-packet. The server sequence
    /// number is only consumed once a full packet is decrypted.
    fn try_read_payload<S>(stream: &mut S, client: &mut Client) -> SshResult<Option<Data>>
    where
        S: Read,
    {
        let bsize = Self::get_align(client.get_encryptor().bsize()) as usize;

        // read (or resume reading) the first block
        if !fill_buf(stream, &mut client.recv_buf, bsize)? {
            return Ok(None);
        }

        // detect the total len; peek the sequence number — the length
        // detection is stateless for every cipher, and we must not burn a
        // sequence number on a packet we may not finish this call
        let seq = client.get_seq().peek_server();
        let first_block = client.recv_buf[..bsize].to_vec();
        let data_len = client.get_encryptor().data_len(seq, &first_block);
        if data_len > MAX_PACKET_WIRE_LEN {
            return Err(SshError::EncryptionError(format!(
                "corrupt packet length {data_len}, transport is desynchronized"
            )));
        }

        // read (or resume reading) the remainder
        if !fill_buf(stream, &mut client.recv_buf, data_len)? {
            return Ok(None);
        }

        // a whole packet is buffered: consume the sequence number and decrypt
        let seq = client.get_seq().get_server();
        let mut data = std::mem::take(&mut client.recv_buf);
        let data = client.get_encryptor().decrypt(seq, &mut data)?;

        // unpacking
        let pkt_len = u32::from_be_bytes(data[0..4].try_into().unwrap());
        let pad_len = data[4];
        let payload_len = pkt_len - pad_len as u32 - 1;

        let payload = data[5..payload_len as usize + 5].into();
        let payload = client.get_compressor().decompress(payload)?.into();

        Ok(Some(payload))
    }

    pub fn from_stream<S>(stream: &mut S, client: &'a mut Client) -> SshResult<Self>
    where
        S: Read,
    {
        let mut timeout = Timeout::new(client.get_timeout());
        loop {
            if let Some(payload) = Self::try_read_payload(stream, client)? {
                return Ok(Self { payload, client });
            }
            timeout.till_next_tick()?;
        }
    }

    pub fn try_from_stream<S>(stream: &mut S, client: &'a mut Client) -> SshResult<Option<Self>>
    where
        S: Read,
    {
        match Self::try_read_payload(stream, client)? {
            Some(payload) => Ok(Some(Self { payload, client })),
            None => Ok(None),
        }
    }

    pub fn get_inner(&self) -> &[u8] {
        &self.payload
    }

    pub fn into_inner(self) -> Data {
        self.payload
    }
}

impl<'a> From<(Data, &'a mut Client)> for SecPacket<'a> {
    fn from((d, c): (Data, &'a mut Client)) -> Self {
        Self {
            payload: d,
            client: c,
        }
    }
}

// ssh4 local modification: regression tests for resumable packet reads.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use std::collections::VecDeque;
    use std::io;

    enum Ev {
        Bytes(Vec<u8>),
        Block,
        Eof,
    }

    /// Scripted stream: yields byte chunks, `WouldBlock`s, or EOF in order.
    /// Once the script is exhausted it keeps returning `WouldBlock`.
    struct Script(VecDeque<Ev>);

    impl io::Read for Script {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            match self.0.front_mut() {
                None => Err(io::ErrorKind::WouldBlock.into()),
                Some(Ev::Block) => {
                    self.0.pop_front();
                    Err(io::ErrorKind::WouldBlock.into())
                }
                Some(Ev::Eof) => Ok(0),
                Some(Ev::Bytes(b)) => {
                    let n = b.len().min(buf.len());
                    buf[..n].copy_from_slice(&b[..n]);
                    b.drain(..n);
                    if b.is_empty() {
                        self.0.pop_front();
                    }
                    Ok(n)
                }
            }
        }
    }

    fn wire_packet(payload: &[u8]) -> Vec<u8> {
        let mut sender = Client::new(Config::default());
        let mut data = Data::new();
        data.extend_from_slice(payload);
        let mut wire = vec![];
        data.pack(&mut sender).write_stream(&mut wire).unwrap();
        wire
    }

    #[test]
    fn packet_split_at_every_byte_boundary_is_resumable() {
        let payload = b"\x5e split me across reads \x07";
        let wire = wire_packet(payload);
        for split in 1..wire.len() {
            let mut client = Client::new(Config::default());
            let mut stream = Script(VecDeque::from([
                Ev::Bytes(wire[..split].to_vec()),
                Ev::Block,
                Ev::Bytes(wire[split..].to_vec()),
            ]));
            // first attempt stalls mid-packet: no packet, no lost bytes
            assert!(
                SecPacket::try_from_stream(&mut stream, &mut client)
                    .unwrap()
                    .is_none(),
                "split {split}: expected incomplete packet"
            );
            // second attempt resumes and completes the same packet
            let pkt = SecPacket::try_from_stream(&mut stream, &mut client)
                .unwrap()
                .expect("packet should complete after resume");
            assert_eq!(pkt.get_inner(), payload, "split {split}");
        }
    }

    #[test]
    fn timeout_midpacket_keeps_partial_bytes() {
        let payload = b"timeout does not desync";
        let wire = wire_packet(payload);
        let mut client = Client::new(Config::default());
        client.set_timeout(Some(Duration::from_millis(5)));

        // only half the packet ever arrives: from_stream must time out...
        let mut stream = Script(VecDeque::from([Ev::Bytes(wire[..wire.len() / 2].to_vec())]));
        let err = SecPacket::from_stream(&mut stream, &mut client)
            .map(|_| ())
            .expect_err("half a packet must not parse");
        assert!(matches!(err, SshError::TimeoutError), "got {err:?}");

        // ...and the partial bytes must survive so the read can resume
        let mut stream = Script(VecDeque::from([Ev::Bytes(wire[wire.len() / 2..].to_vec())]));
        let pkt = SecPacket::from_stream(&mut stream, &mut client).unwrap();
        assert_eq!(pkt.get_inner(), payload);
    }

    #[test]
    fn eof_midpacket_is_an_error_not_a_hang() {
        let wire = wire_packet(b"closed early");
        let mut client = Client::new(Config::default());
        let mut stream = Script(VecDeque::from([Ev::Bytes(wire[..5].to_vec()), Ev::Eof]));
        let err = SecPacket::try_from_stream(&mut stream, &mut client)
            .map(|_| ())
            .expect_err("eof mid-packet must error");
        match err {
            SshError::IoError(e) => assert_eq!(e.kind(), io::ErrorKind::UnexpectedEof),
            other => panic!("expected unexpected-eof error, got {other:?}"),
        }
    }

    #[test]
    fn back_to_back_packets_parse_independently() {
        let mut sender = Client::new(Config::default());
        let mut wire = vec![];
        for payload in [b"first".as_slice(), b"second".as_slice()] {
            let mut data = Data::new();
            data.extend_from_slice(payload);
            data.pack(&mut sender).write_stream(&mut wire).unwrap();
        }
        let mut client = Client::new(Config::default());
        let mut stream = Script(VecDeque::from([Ev::Bytes(wire)]));
        let p1 = SecPacket::try_from_stream(&mut stream, &mut client)
            .unwrap()
            .unwrap()
            .into_inner();
        assert_eq!(&*p1, b"first");
        let p2 = SecPacket::try_from_stream(&mut stream, &mut client)
            .unwrap()
            .unwrap()
            .into_inner();
        assert_eq!(&*p2, b"second");
    }
}
