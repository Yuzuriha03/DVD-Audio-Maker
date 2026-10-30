use std::borrow::Cow;

use crate::format::Profile;

#[derive(Clone, Debug)]
pub struct Config {
    pub sample_rate: u32,
    pub bits: u8,
    pub channels: u8,
    pub restart_interval: usize,
    /// Explicit framed metadata packet (length BE16, 0x40, 0, payload).
    pub metadata: Vec<u8>,
}

/// Configuration admitted by the batch API, including its PCM shape checks.
pub(crate) struct BatchConfig<'a> {
    config: Cow<'a, Config>,
    samples_per_access_unit: u32,
}

impl<'a> BatchConfig<'a> {
    pub(crate) fn new(
        config: &'a Config,
        profile: &Profile,
        pcm_len: usize,
    ) -> Result<Self, Error> {
        let mut config = Cow::Borrowed(config);
        if config.restart_interval == 0 {
            config.to_mut().restart_interval = 8;
        }
        if !profile.valid(&config) {
            return Err(Error::InvalidConfig);
        }
        if !(1..=6).contains(&config.channels)
            || ![16, 20, 24].contains(&config.bits)
            || !(1..=32).contains(&config.restart_interval)
            || pcm_len == 0
            || !pcm_len.is_multiple_of(usize::from(config.channels))
        {
            return Err(Error::InvalidConfig);
        }
        let shift = match config.sample_rate {
            44100 | 48000 => 0,
            88200 | 96000 => 1,
            176400 | 192000 => 2,
            _ => return Err(Error::InvalidConfig),
        };
        if shift == 2 && config.channels > 2 {
            return Err(Error::InvalidConfig);
        }
        Ok(Self {
            config,
            samples_per_access_unit: 40 << shift,
        })
    }

    pub(crate) fn config(&self) -> &Config {
        &self.config
    }

    pub(crate) fn samples_per_access_unit(&self) -> u32 {
        self.samples_per_access_unit
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidConfig,
    InvalidMetadata,
    InvalidPcm,
    AccessUnitTooLarge,
    FifoOverflow,
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_admission_normalizes_without_mutating_input() {
        let c = Config {
            sample_rate: 48000,
            bits: 24,
            channels: 2,
            restart_interval: 0,
            metadata: vec![0, 0, 0x40, 0],
        };
        let profile = Profile::standard(&c);
        let admitted = BatchConfig::new(&c, &profile, 2).unwrap();
        assert_eq!(admitted.config().restart_interval, 8);
        assert_eq!(admitted.samples_per_access_unit(), 40);
        assert_eq!(c.restart_interval, 0);
        assert_eq!(admitted.config().metadata, c.metadata);
        for length in [0, 1, 3] {
            assert!(matches!(
                BatchConfig::new(&c, &profile, length),
                Err(Error::InvalidConfig)
            ));
        }
        let mut c = c;
        c.restart_interval = 8;
        let admitted = BatchConfig::new(&c, &profile, 2).unwrap();
        assert!(std::ptr::eq(admitted.config(), &c));
        c.channels = 0;
        assert!(matches!(
            BatchConfig::new(&c, &profile, 2),
            Err(Error::InvalidConfig)
        ));
    }
}
