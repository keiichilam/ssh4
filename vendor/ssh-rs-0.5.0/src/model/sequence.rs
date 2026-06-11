use super::U32Iter;

#[derive(Default)]
pub(crate) struct Sequence {
    client_sequence_num: U32Iter,
    server_sequence_num: U32Iter,
}

impl Sequence {
    pub fn get_client(&mut self) -> u32 {
        self.client_sequence_num.next().unwrap()
    }

    pub fn get_server(&mut self) -> u32 {
        self.server_sequence_num.next().unwrap()
    }

    /// ssh4 local modification: the value the next `get_server` will return,
    /// without consuming it (used to detect a packet length before the whole
    /// packet has arrived).
    pub fn peek_server(&self) -> u32 {
        self.server_sequence_num.peek()
    }

    pub fn new() -> Self {
        Self {
            ..Default::default()
        }
    }
}
