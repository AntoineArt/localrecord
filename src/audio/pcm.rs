/// Convert little-endian f32 PCM, dropping NaN/Inf that WASAPI can emit as garbage.
pub fn f32_from_le_bytes(bytes: [u8; 4]) -> f32 {
    let sample = f32::from_le_bytes(bytes);
    if sample.is_finite() {
        sample.clamp(-1.0, 1.0)
    } else {
        0.0
    }
}

pub fn bytes_to_f32(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32_from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

/// Append one WASAPI packet. Silent packets must be zeros: the buffer contents
/// are undefined and sound like static if mixed in.
pub fn append_packet_samples(pending: &mut Vec<f32>, bytes: &[u8], silent: bool) {
    let n = bytes.len() / 4;
    if silent {
        pending.resize(pending.len() + n, 0.0);
        return;
    }
    pending.extend(bytes_to_f32(bytes));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_packet_is_zeros_not_buffer_garbage() {
        let garbage = 0.75f32.to_le_bytes();
        let mut pending = Vec::new();
        append_packet_samples(&mut pending, &garbage, true);
        assert_eq!(pending, vec![0.0]);
    }

    #[test]
    fn audible_packet_keeps_samples() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&0.5f32.to_le_bytes());
        bytes.extend_from_slice(&(-0.25f32).to_le_bytes());
        let mut pending = Vec::new();
        append_packet_samples(&mut pending, &bytes, false);
        assert!((pending[0] - 0.5).abs() < f32::EPSILON);
        assert!((pending[1] + 0.25).abs() < f32::EPSILON);
    }

    #[test]
    fn nan_and_inf_become_silence() {
        assert_eq!(f32_from_le_bytes(f32::NAN.to_le_bytes()), 0.0);
        assert_eq!(f32_from_le_bytes(f32::INFINITY.to_le_bytes()), 0.0);
        assert_eq!(f32_from_le_bytes(f32::NEG_INFINITY.to_le_bytes()), 0.0);
    }

    #[test]
    fn out_of_range_samples_are_clamped() {
        assert_eq!(f32_from_le_bytes(2.0f32.to_le_bytes()), 1.0);
        assert_eq!(f32_from_le_bytes((-3.0f32).to_le_bytes()), -1.0);
    }
}
