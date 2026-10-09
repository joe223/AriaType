use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use tracing::{info, warn};

/// Audio and immutable format metadata owned by one recording session.
/// Keeping this value in the consumer preserves retry audio after capture stops.
#[derive(Clone)]
pub struct RecordedAudio {
    path: PathBuf,
    samples: Arc<Mutex<Vec<i16>>>,
    sample_rate: u32,
    channels: u16,
}

impl RecordedAudio {
    pub fn new(
        path: PathBuf,
        samples: Arc<Mutex<Vec<i16>>>,
        sample_rate: u32,
        channels: u16,
    ) -> Self {
        Self {
            path,
            samples,
            sample_rate,
            channels,
        }
    }
}

/// Save this session's raw PCM to WAV, independently of active recording state.
/// Returns the path to the saved file, or None if no audio was recorded.
pub fn save_raw_audio_to_file(audio: &RecordedAudio) -> Option<String> {
    let path = &audio.path;
    let sample_rate = audio.sample_rate;
    let channels = audio.channels;
    let buffer = audio.samples.lock();

    if buffer.is_empty() {
        warn!("raw_audio_buffer_empty-no_file_saved");
        return None;
    }

    if sample_rate == 0 || channels == 0 {
        warn!(
            sample_rate,
            channels, "audio_params_not_set-cannot_save_file"
        );
        return None;
    }

    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    match hound::WavWriter::create(path, spec) {
        Ok(mut writer) => {
            for sample in buffer.iter() {
                if let Err(e) = writer.write_sample(*sample) {
                    warn!(error = %e, "wav_write_failed");
                    return None;
                }
            }
            if let Err(e) = writer.finalize() {
                warn!(error = %e, "wav_finalize_failed");
                return None;
            }
            info!(path = %path.display(), samples = buffer.len(), sample_rate, channels, "raw_audio_saved");
            Some(path.to_string_lossy().to_string())
        }
        Err(e) => {
            warn!(error = %e, path = %path.display(), "wav_create_failed");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{save_raw_audio_to_file, RecordedAudio};
    use parking_lot::Mutex;
    use std::sync::Arc;

    #[test]
    fn stopped_recording_preserves_its_audio_and_format_after_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let original_path = directory.path().join("original.wav");
        let mut active_audio = Some(RecordedAudio::new(
            original_path.clone(),
            Arc::new(Mutex::new(vec![100, 200, 300, 400])),
            16_000,
            1,
        ));
        let consumer_audio = active_audio.as_ref().unwrap().clone();
        drop(active_audio.take());
        active_audio = Some(RecordedAudio::new(
            directory.path().join("newer.wav"),
            Arc::new(Mutex::new(vec![900, 901])),
            48_000,
            2,
        ));

        let saved = save_raw_audio_to_file(&consumer_audio).unwrap();
        assert_eq!(saved, original_path.to_string_lossy());
        let mut reader = hound::WavReader::open(&original_path).unwrap();
        assert_eq!(reader.spec().sample_rate, 16_000);
        assert_eq!(reader.spec().channels, 1);
        assert_eq!(
            reader
                .samples::<i16>()
                .map(Result::unwrap)
                .collect::<Vec<_>>(),
            vec![100, 200, 300, 400]
        );
        assert!(!active_audio.unwrap().path.exists());
    }

    #[test]
    fn empty_or_invalid_recording_does_not_create_retry_audio() {
        let directory = tempfile::tempdir().unwrap();
        for (name, samples, sample_rate, channels) in [
            ("empty.wav", vec![], 16_000, 1),
            ("invalid_rate.wav", vec![1], 0, 1),
            ("invalid_channels.wav", vec![1], 16_000, 0),
        ] {
            let path = directory.path().join(name);
            let audio = RecordedAudio::new(
                path.clone(),
                Arc::new(Mutex::new(samples)),
                sample_rate,
                channels,
            );
            assert_eq!(save_raw_audio_to_file(&audio), None);
            assert!(!path.exists());
        }
    }
}
