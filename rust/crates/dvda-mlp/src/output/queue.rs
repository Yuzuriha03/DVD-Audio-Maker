use std::collections::VecDeque;

use crate::timing::{Descriptor, LOOKAHEAD, Timing};

struct Node {
    words: Vec<u32>,
    descriptor: Descriptor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueueFailure {
    Allocation,
    Timing,
    Output,
}

pub(crate) struct MlpOutputQueue {
    timing: Timing,
    nodes: VecDeque<Node>,
    primed: usize,
    rate: u32,
    substreams: u32,
    decode: u32,
    failure: Option<QueueFailure>,
    finished: bool,
}

impl MlpOutputQueue {
    pub(crate) fn mlp_output_queue_init() -> Self {
        Self {
            timing: Timing::new(),
            nodes: VecDeque::with_capacity(LOOKAHEAD + 1),
            primed: 0,
            rate: 0,
            substreams: 0,
            decode: 0,
            failure: None,
            finished: false,
        }
    }

    pub(crate) fn mlp_output_queue_dispose(&mut self) {
        self.nodes.clear();
        self.finished = true;
    }

    fn release(
        &mut self,
        next: Descriptor,
        emit: &mut impl FnMut(&[u32]) -> bool,
    ) -> Result<(), QueueFailure> {
        let node = self.nodes.front_mut().expect("queued output node");
        self.timing
            .step(next, &mut node.descriptor)
            .map_err(|_| QueueFailure::Timing)?;
        node.words[0] = node.descriptor.header;
        node.words[1] = node.descriptor.arrival;
        if !emit(&node.words) {
            return Err(QueueFailure::Output);
        }
        self.nodes.pop_front();
        Ok(())
    }

    pub(crate) fn mlp_output_queue_push(
        &mut self,
        words: &[u32],
        samples: u32,
        emit: &mut impl FnMut(&[u32]) -> bool,
    ) -> bool {
        let count = words.len();
        if self.failure.is_some()
            || self.finished
            || samples == 0
            || samples > 160
            || !(4..=4095).contains(&count)
            || words[0] & 4095 != count as u32
            || self.nodes.len() > LOOKAHEAD
        {
            return false;
        }
        let major = words[2] == 0xf872 && words[3] == 0x6fbb;
        let directory = if major { 16 } else { 2 };
        let (rate, substreams) = if major {
            if count < 18 {
                return false;
            }
            (words[9], words[10] >> 12)
        } else {
            (self.rate, self.substreams)
        };
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
            self.failure = Some(QueueFailure::Allocation);
            return false;
        }
        copy.extend_from_slice(words);
        self.nodes.push_back(Node {
            words: copy,
            descriptor: d,
        });
        self.rate = rate;
        self.substreams = substreams;
        self.decode = (self.decode + samples) & 65535;
        if self.primed < LOOKAHEAD {
            if self.timing.step(d, &mut Descriptor::default()).is_err() {
                self.failure = Some(QueueFailure::Timing);
                return false;
            }
            self.primed += 1;
        } else if let Err(failure) = self.release(d, emit) {
            self.failure = Some(failure);
            return false;
        }
        true
    }

    pub(crate) fn mlp_output_queue_finish(
        &mut self,
        emit: &mut impl FnMut(&[u32]) -> bool,
    ) -> bool {
        if self.failure.is_some() || self.finished {
            return false;
        }
        while self.primed < LOOKAHEAD {
            if self
                .timing
                .step(Descriptor::default(), &mut Descriptor::default())
                .is_err()
            {
                self.failure = Some(QueueFailure::Timing);
                return false;
            }
            self.primed += 1;
        }
        while !self.nodes.is_empty() {
            if let Err(failure) = self.release(Descriptor::default(), emit) {
                self.failure = Some(failure);
                return false;
            }
        }
        self.finished = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn major_unit() -> [u32; 18] {
        let mut words = [0; 18];
        words[0] = 18;
        words[2] = 0xf872;
        words[3] = 0x6fbb;
        words[9] = 0x8100;
        words[10] = 1 << 12;
        words[16] = 1;
        words
    }

    #[test]
    fn empty_finish_is_terminal() {
        let mut queue = MlpOutputQueue::mlp_output_queue_init();
        let mut emit = |_: &[u32]| panic!("empty queue emitted output");
        assert!(queue.mlp_output_queue_finish(&mut emit));
        assert!(!queue.mlp_output_queue_finish(&mut emit));
        assert!(!queue.mlp_output_queue_push(&major_unit(), 40, &mut emit));
    }

    #[test]
    fn emit_failure_retains_node_until_disposal() {
        let mut queue = MlpOutputQueue::mlp_output_queue_init();
        let words = major_unit();
        assert!(queue.mlp_output_queue_push(&words, 40, &mut |_| true));
        let mut calls = 0;
        assert!(!queue.mlp_output_queue_finish(&mut |output| {
            calls += 1;
            assert_eq!(&output[2..], &words[2..]);
            false
        }));
        assert_eq!(calls, 1);
        assert_eq!(queue.nodes.len(), 1);
        assert_eq!(queue.failure, Some(QueueFailure::Output));
        let mut emit = |_: &[u32]| panic!("failed queue emitted output");
        assert!(!queue.mlp_output_queue_finish(&mut emit));
        assert!(!queue.mlp_output_queue_push(&words, 40, &mut emit));
        queue.mlp_output_queue_dispose();
        assert!(queue.nodes.is_empty());
        assert!(queue.finished);
    }

    #[test]
    fn timing_failure_preserves_prior_output_and_retains_failed_node() {
        let mut queue = MlpOutputQueue::mlp_output_queue_init();
        let mut words = major_unit().to_vec();
        words.resize(4095, 0);
        words[0] = 4095;
        words[9] = 1;
        words[16] = 4095 - 17;
        assert!(queue.mlp_output_queue_push(&words, 1, &mut |_| {
            panic!("output before lookahead is filled")
        }));
        assert!(queue.mlp_output_queue_push(&words, 1, &mut |_| true));
        let mut emitted = 0;
        assert!(!queue.mlp_output_queue_finish(&mut |_| {
            emitted += 1;
            true
        }));
        assert_eq!(emitted, 1);
        assert_eq!(queue.failure, Some(QueueFailure::Timing));
        assert_eq!(queue.nodes.len(), 1);
        assert_eq!(queue.nodes.front().unwrap().words, words);
        assert_eq!(queue.decode, 2);
        assert_eq!(queue.rate, 1);
        assert!(!queue.finished);
        queue.mlp_output_queue_dispose();
        assert!(queue.nodes.is_empty());
        assert!(queue.finished);
        assert_eq!(queue.failure, Some(QueueFailure::Timing));
    }

    #[test]
    fn lookahead_wrap_preserves_fifo_order() {
        let mut queue = MlpOutputQueue::mlp_output_queue_init();
        let mut emitted = Vec::new();
        let mut emit = |words: &[u32]| {
            emitted.push(*words.last().unwrap());
            true
        };
        let mut major = major_unit();
        major[17] = 0;
        assert!(queue.mlp_output_queue_push(&major, 40, &mut emit));
        let count = LOOKAHEAD * 3 + 7;
        for index in 1..count {
            let words = [4, 0, 1, index as u32];
            assert!(queue.mlp_output_queue_push(&words, 40, &mut emit));
            assert_eq!(queue.nodes.len(), (index + 1).min(LOOKAHEAD));
        }
        assert!(queue.mlp_output_queue_finish(&mut emit));
        assert!(queue.nodes.is_empty());
        assert_eq!(emitted, (0..count as u32).collect::<Vec<_>>());
    }

    #[test]
    fn invalid_major_preserves_committed_stream_format() {
        let mut queue = MlpOutputQueue::mlp_output_queue_init();
        assert!(queue.mlp_output_queue_push(&major_unit(), 40, &mut |_| true));
        let mut invalid = major_unit();
        invalid[9] = 0x8200;
        invalid[10] = 2 << 12;
        assert!(!queue.mlp_output_queue_push(&invalid, 80, &mut |_| true));
        assert_eq!(queue.rate, 0x8100);
        assert_eq!(queue.substreams, 1);
        assert_eq!(queue.decode, 40);
        assert_eq!(queue.primed, 1);
        assert_eq!(queue.nodes.len(), 1);
        assert_eq!(queue.failure, None);
        assert!(queue.mlp_output_queue_push(&[4, 0, 1, 7], 40, &mut |_| true));
        assert!(queue.mlp_output_queue_finish(&mut |_| true));
    }

    #[test]
    fn push_emit_failure_retains_both_front_and_new_node() {
        let mut queue = MlpOutputQueue::mlp_output_queue_init();
        assert!(queue.mlp_output_queue_push(&major_unit(), 40, &mut |_| true));
        for index in 1..LOOKAHEAD {
            assert!(
                queue.mlp_output_queue_push(&[4, 0, 1, index as u32], 40, &mut |_| {
                    panic!("output before lookahead is filled")
                })
            );
        }
        let mut calls = 0;
        assert!(
            !queue.mlp_output_queue_push(&[4, 0, 1, 123], 80, &mut |words| {
                calls += 1;
                assert_eq!(&words[2..], &major_unit()[2..]);
                false
            })
        );
        assert_eq!(calls, 1);
        assert_eq!(queue.failure, Some(QueueFailure::Output));
        assert_eq!(queue.nodes.len(), LOOKAHEAD + 1);
        assert_eq!(queue.nodes.back().unwrap().words, [4, 0, 1, 123]);
        assert_eq!(queue.decode, (LOOKAHEAD as u32 * 40 + 80) & 65535);
        assert_eq!(queue.primed, LOOKAHEAD);
        assert!(!queue.mlp_output_queue_finish(&mut |_| panic!("failed queue emitted")));
        queue.mlp_output_queue_dispose();
        queue.mlp_output_queue_dispose();
        assert!(queue.nodes.is_empty());
        assert_eq!(queue.failure, Some(QueueFailure::Output));
        assert!(queue.finished);
    }

    #[test]
    fn rejected_input_does_not_poison_queue() {
        let mut queue = MlpOutputQueue::mlp_output_queue_init();
        for words in [&[][..], &[4, 0, 1, 0][..], &[18, 0, 0xf872, 0x6fbb][..]] {
            assert!(!queue.mlp_output_queue_push(words, 40, &mut |_| true));
            assert!(queue.nodes.is_empty());
            assert_eq!(queue.failure, None);
        }
        assert!(queue.mlp_output_queue_push(&major_unit(), 40, &mut |_| true));
        assert!(queue.mlp_output_queue_finish(&mut |_| true));
    }
}
