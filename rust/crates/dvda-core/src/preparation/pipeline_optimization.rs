// Pipeline optimization with overlapping stages
// Implements producer-consumer pattern for overlapping I/O and computation

use std::sync::mpsc::{Receiver, Sender, sync_channel};
use std::thread;

/// Pipeline stage result
pub enum StageResult<T> {
    Data(T),
    Done,
    Error(String),
}

/// Create a channel for communicating explicit pipeline stage events.
///
/// ```
/// use dvda_core::preparation::pipeline_optimization::{create_pipeline, StageResult};
/// let (producer, consumer) = create_pipeline();
/// producer.send(StageResult::Data(42)).unwrap();
/// assert!(matches!(consumer.recv().unwrap(), StageResult::Data(42)));
/// ```
pub fn create_pipeline<T>() -> (Sender<StageResult<T>>, Receiver<StageResult<T>>) {
    std::sync::mpsc::channel()
}

/// Pipeline configuration
pub struct PipelineConfig {
    /// Buffer size for each stage
    pub buffer_size: usize,

    /// Enable pipeline optimization
    pub enabled: bool,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            buffer_size: 8,
            enabled: true,
        }
    }
}

/// Multi-stage pipeline builder
///
/// Supports overlapping:
/// - Stage 1: Read from disk
/// - Stage 2: Decode audio
/// - Stage 3: Encode MLP
/// - Stage 4: Write to cache
pub struct Pipeline<T> {
    stages: Vec<Box<dyn FnMut(T) -> T + Send>>,
    config: PipelineConfig,
}

impl<T: Send + 'static> Pipeline<T> {
    pub fn new(config: PipelineConfig) -> Self {
        Self {
            stages: Vec::new(),
            config,
        }
    }

    /// Add a stage to the pipeline
    pub fn add_stage<F>(mut self, stage: F) -> Self
    where
        F: FnMut(T) -> T + Send + 'static,
    {
        self.stages.push(Box::new(stage));
        self
    }

    /// Execute the pipeline.
    ///
    /// An enabled pipeline owns one worker thread per stage. Each adjacent
    /// pair of workers is connected by a bounded channel, so a fast producer
    /// cannot grow an unbounded queue while a later stage is busy. Every stage
    /// has one receiver and one sender, which keeps the input order; the
    /// explicit sequence number also makes that guarantee visible at the
    /// final collection point.
    ///
    /// With no stages, inputs pass through unchanged. A disabled pipeline
    /// applies all stages on the calling thread, preserving the same stage
    /// order without spawning workers.
    pub fn execute(self, inputs: Vec<T>) -> Vec<T> {
        if self.stages.is_empty() || inputs.is_empty() {
            return inputs;
        }

        if !self.config.enabled {
            return self.stages.into_iter().fold(inputs, |items, mut stage| {
                items.into_iter().map(&mut stage).collect()
            });
        }

        let input_count = inputs.len();
        let capacity = self.config.buffer_size.max(1);
        let (first_tx, first_rx) = sync_channel(capacity);
        let mut current_rx = first_rx;
        let mut workers = Vec::with_capacity(self.stages.len());

        for mut stage in self.stages {
            let upstream = current_rx;
            let (next_tx, next_rx) = sync_channel(capacity);
            workers.push(thread::spawn(move || {
                for (sequence, item) in upstream {
                    let item = stage(item);
                    if next_tx.send((sequence, item)).is_err() {
                        break;
                    }
                }
            }));
            current_rx = next_rx;
        }

        // Feeding must overlap draining as well: otherwise a batch larger than
        // all bounded queues can fill the final queue and deadlock its producer.
        let producer = thread::spawn(move || {
            for (sequence, item) in inputs.into_iter().enumerate() {
                if first_tx.send((sequence, item)).is_err() {
                    break;
                }
            }
        });

        let mut ordered = (0..input_count).map(|_| None).collect::<Vec<Option<T>>>();
        for (sequence, item) in current_rx {
            ordered[sequence] = Some(item);
        }

        for worker in workers {
            worker
                .join()
                .expect("pipeline stage panicked while processing input");
        }
        producer.join().expect("pipeline input producer panicked");

        ordered
            .into_iter()
            .map(|item| item.expect("pipeline stage dropped an input item"))
            .collect()
    }
}

/// Example: 3-stage pipeline for audio processing
///
/// ```rust
/// let pipeline = Pipeline::new(PipelineConfig::default())
///     .add_stage(|path| read_audio_file(path))
///     .add_stage(|data| decode_audio(data))
///     .add_stage(|pcm| encode_mlp(pcm));
///
/// let results = pipeline.execute(file_paths);
/// ```
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_config() {
        let config = PipelineConfig::default();
        assert!(config.enabled);
        assert_eq!(config.buffer_size, 8);
    }

    #[test]
    fn test_stage_channel() {
        let (tx, rx) = create_pipeline();
        tx.send(StageResult::Data(42)).unwrap();

        match rx.recv().unwrap() {
            StageResult::Data(n) => assert_eq!(n, 42),
            _ => panic!("Expected data"),
        }
    }

    #[test]
    fn enabled_pipeline_overlaps_stages_and_preserves_input_order() {
        let (later_stage_started, wait_for_later_stage) = std::sync::mpsc::channel();
        let config = PipelineConfig {
            buffer_size: 1,
            enabled: true,
        };
        let pipeline = Pipeline::new(config)
            .add_stage(move |value| {
                if value == 1 {
                    wait_for_later_stage
                        .recv_timeout(std::time::Duration::from_secs(2))
                        .expect(
                            "the next stage must run before the first stage finishes all inputs",
                        );
                }
                value + 1
            })
            .add_stage(move |value| {
                if value == 1 {
                    later_stage_started.send(()).unwrap();
                }
                value * 2
            })
            .add_stage(|value| value - 3);

        assert_eq!(pipeline.execute(vec![0, 1, 2, 3, 4]), vec![-1, 1, 3, 5, 7]);
    }

    #[test]
    fn bounded_pipeline_drains_batches_larger_than_its_queues() {
        let config = PipelineConfig {
            buffer_size: 0,
            enabled: true,
        };
        let mut sequence = 0;
        let pipeline = Pipeline::new(config)
            .add_stage(move |value| {
                assert_eq!(value, sequence);
                sequence += 1;
                value + 1
            })
            .add_stage(|value| value * 2);
        assert_eq!(
            pipeline.execute((0..10_000).collect()),
            (0..10_000).map(|value| (value + 1) * 2).collect::<Vec<_>>()
        );
    }

    #[test]
    fn disabled_pipeline_applies_stages_serially() {
        let calling_thread = std::thread::current().id();
        let config = PipelineConfig {
            buffer_size: 0,
            enabled: false,
        };
        let pipeline = Pipeline::new(config)
            .add_stage(move |value| {
                assert_eq!(std::thread::current().id(), calling_thread);
                value + 10
            })
            .add_stage(|value| value / 2);

        assert_eq!(pipeline.execute(vec![2, 4, 6]), vec![6, 7, 8]);
    }

    #[test]
    fn pipeline_without_stages_is_a_passthrough() {
        let pipeline = Pipeline::<String>::new(PipelineConfig::default());
        assert_eq!(
            pipeline.execute(vec!["a".into(), "b".into()]),
            vec!["a", "b"]
        );
    }
}
