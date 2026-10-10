use crate::timing::{Descriptor, LOOKAHEAD, Timing};

struct Node {
    words: Vec<u32>,
    descriptor: Descriptor,
}

pub(crate) struct MlpOutputQueue {
    timing: Timing,
    nodes: Vec<Option<Node>>,
    read: usize,
    write: usize,
    count: usize,
    primed: usize,
    rate: u32,
    substreams: u32,
    decode: u32,
    failed: bool,
    finished: bool,
}

impl MlpOutputQueue {
    #[cfg(test)]
    pub(crate) fn state(&self) -> [u32; 9] {
        [
            self.read as u32,
            self.write as u32,
            self.count as u32,
            self.primed as u32,
            self.rate,
            self.substreams,
            self.decode,
            u32::from(self.failed),
            u32::from(self.finished),
        ]
    }

    pub(crate) fn mlp_output_queue_init() -> Self {
        Self {
            timing: Timing::new(),
            nodes: (0..=LOOKAHEAD).map(|_| None).collect(),
            read: 0,
            write: 0,
            count: 0,
            primed: 0,
            rate: 0,
            substreams: 0,
            decode: 0,
            failed: false,
            finished: false,
        }
    }

    pub(crate) fn mlp_output_queue_dispose(&mut self) {
        for node in &mut self.nodes {
            *node = None;
        }
        self.count = 0;
        self.finished = true;
    }

    fn release(&mut self, next: Descriptor, emit: &mut impl FnMut(&[u32]) -> bool) -> bool {
        let node = self.nodes[self.read].as_mut().expect("queued output node");
        if self.timing.step(next, &mut node.descriptor).is_err() {
            return false;
        }
        node.words[0] = node.descriptor.header;
        node.words[1] = node.descriptor.arrival;
        if !emit(&node.words) {
            return false;
        }
        self.nodes[self.read] = None;
        self.read = (self.read + 1) % (LOOKAHEAD + 1);
        self.count -= 1;
        true
    }

    pub(crate) fn mlp_output_queue_push(
        &mut self,
        words: &[u32],
        samples: u32,
        emit: &mut impl FnMut(&[u32]) -> bool,
    ) -> bool {
        let count = words.len();
        if self.failed
            || self.finished
            || samples == 0
            || samples > 160
            || !(4..=4095).contains(&count)
            || words[0] & 4095 != count as u32
            || self.count > LOOKAHEAD
        {
            return false;
        }
        let major = words[2] == 0xf872 && words[3] == 0x6fbb;
        let (mut rate, mut substreams) = (self.rate, self.substreams);
        let directory = if major { 16 } else { 2 };
        if major {
            if count < 18 {
                return false;
            }
            rate = words[9];
            substreams = words[10] >> 12;
        }
        if rate & 0x7fff == 0
            || !(1..=2).contains(&substreams)
            || count <= directory + substreams as usize
        {
            return false;
        }
        let first = words[directory] & 4095;
        let last = words[directory + substreams as usize - 1] & 4095;
        if first == 0 || first > last || last as usize + directory + substreams as usize != count {
            return false;
        }
        let d = Descriptor {
            rate,
            samples,
            decode: self.decode,
            words: count as u32,
            groups: [first, last - first],
            arrival: words[1],
            header: words[0],
        };
        let mut copy = Vec::new();
        if copy.try_reserve_exact(count).is_err() {
            self.failed = true;
            return false;
        }
        copy.extend_from_slice(words);
        self.nodes[self.write] = Some(Node {
            words: copy,
            descriptor: d,
        });
        self.rate = rate;
        self.substreams = substreams;
        self.decode = (self.decode + samples) & 65535;
        self.write = (self.write + 1) % (LOOKAHEAD + 1);
        self.count += 1;
        if self.primed < LOOKAHEAD {
            if self.timing.step(d, &mut Descriptor::default()).is_err() {
                self.failed = true;
                return false;
            }
            self.primed += 1;
        } else if !self.release(d, emit) {
            self.failed = true;
            return false;
        }
        true
    }

    pub(crate) fn mlp_output_queue_finish(
        &mut self,
        emit: &mut impl FnMut(&[u32]) -> bool,
    ) -> bool {
        if self.failed || self.finished {
            return false;
        }
        while self.primed < LOOKAHEAD {
            if self
                .timing
                .step(Descriptor::default(), &mut Descriptor::default())
                .is_err()
            {
                self.failed = true;
                return false;
            }
            self.primed += 1;
        }
        while self.count != 0 {
            if !self.release(Descriptor::default(), emit) {
                self.failed = true;
                return false;
            }
        }
        self.finished = true;
        true
    }
}
