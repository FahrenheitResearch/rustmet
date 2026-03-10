use super::parser::{DataRepresentation, Grib2Message};

/// Bit reader for extracting packed values from GRIB2 data sections.
pub struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        BitReader { data, bit_pos: 0 }
    }

    /// Read `n` bits as an unsigned integer (up to 64 bits).
    pub fn read_bits(&mut self, n: usize) -> u64 {
        if n == 0 {
            return 0;
        }
        let mut result: u64 = 0;
        for _ in 0..n {
            let byte_idx = self.bit_pos / 8;
            let bit_idx = 7 - (self.bit_pos % 8);
            if byte_idx < self.data.len() {
                result = (result << 1) | ((self.data[byte_idx] >> bit_idx) as u64 & 1);
            } else {
                result <<= 1;
            }
            self.bit_pos += 1;
        }
        result
    }

    /// Read `n` bits as a signed integer using sign-magnitude convention.
    /// MSB = sign (1 = negative), remaining bits = magnitude.
    pub fn read_signed_bits(&mut self, n: usize) -> i64 {
        if n == 0 {
            return 0;
        }
        if n == 1 {
            let _bit = self.read_bits(1);
            return 0;
        }
        let sign = self.read_bits(1);
        let magnitude = self.read_bits(n - 1) as i64;
        if sign == 1 {
            -magnitude
        } else {
            magnitude
        }
    }

    /// Align the bit position to the next byte boundary.
    pub fn align_to_byte(&mut self) {
        let rem = self.bit_pos % 8;
        if rem != 0 {
            self.bit_pos += 8 - rem;
        }
    }

    /// Number of bits remaining.
    pub fn remaining_bits(&self) -> usize {
        let total = self.data.len() * 8;
        if self.bit_pos >= total {
            0
        } else {
            total - self.bit_pos
        }
    }
}

/// Unpack a GRIB2 message's data section to floating-point values.
pub fn unpack_message(msg: &Grib2Message) -> crate::error::Result<Vec<f64>> {
    let dr = &msg.data_rep;

    let values = match dr.template {
        0 => unpack_simple(&msg.raw_data, dr).map_err(crate::RustmetError::Unpack)?,
        2 => unpack_complex(&msg.raw_data, dr).map_err(crate::RustmetError::Unpack)?,
        3 => unpack_complex_spatial(&msg.raw_data, dr).map_err(crate::RustmetError::Unpack)?,
        40 => unpack_jpeg2000(&msg.raw_data, dr).map_err(crate::RustmetError::Unpack)?,
        41 => unpack_png(&msg.raw_data, dr).map_err(crate::RustmetError::Unpack)?,
        _ => {
            return Err(crate::RustmetError::UnsupportedTemplate {
                template: dr.template,
                detail: "data representation template".to_string(),
            })
        }
    };

    // Apply bitmap if present
    let mut values = if let Some(ref bitmap) = msg.bitmap {
        let n = bitmap.len();
        let mut result = vec![f64::NAN; n];
        let mut val_idx = 0;
        for i in 0..n {
            if bitmap[i] {
                if val_idx < values.len() {
                    result[i] = values[val_idx];
                    val_idx += 1;
                }
            }
        }
        result
    } else {
        values
    };

    // Apply scan mode orientation correction.
    // HRRR typically has scan_mode 0x40 (bit 6 set) meaning +j direction (south-to-north).
    // We normalize to north-to-south (top-down) row order for rendering.
    let scan_mode = msg.grid.scan_mode;
    let nx = msg.grid.nx as usize;
    let ny = msg.grid.ny as usize;
    if nx > 0 && ny > 0 && values.len() == nx * ny {
        // Bit 6 (0x40): j-direction positive = south-to-north → flip rows
        if scan_mode & 0x40 != 0 {
            for j in 0..ny / 2 {
                let j_rev = ny - 1 - j;
                for i in 0..nx {
                    values.swap(j * nx + i, j_rev * nx + i);
                }
            }
        }
    }

    Ok(values)
}

/// Apply the GRIB2 scaling formula: Y = (R + X * 2^E) * 10^(-D)
fn apply_scaling(raw: &[i64], dr: &DataRepresentation) -> Vec<f64> {
    let r = dr.reference_value as f64;
    let e = dr.binary_scale as f64;
    let d = dr.decimal_scale as f64;
    let two_e = 2.0_f64.powf(e);
    let ten_neg_d = 10.0_f64.powf(-d);

    raw.iter()
        .map(|&x| (r + x as f64 * two_e) * ten_neg_d)
        .collect()
}

/// Template 5.0: Simple packing.
fn unpack_simple(data: &[u8], dr: &DataRepresentation) -> Result<Vec<f64>, String> {
    let bpv = dr.bits_per_value as usize;
    if bpv == 0 {
        // All values are the reference value
        let n = if !data.is_empty() { 1 } else { 0 };
        return Ok(vec![dr.reference_value as f64; n]);
    }

    let total_bits = data.len() * 8;
    let n = total_bits / bpv;
    let mut reader = BitReader::new(data);
    let mut raw = Vec::with_capacity(n);
    for _ in 0..n {
        raw.push(reader.read_bits(bpv) as i64);
    }

    Ok(apply_scaling(&raw, dr))
}

/// Template 5.2: Complex packing.
fn unpack_complex(data: &[u8], dr: &DataRepresentation) -> Result<Vec<f64>, String> {
    let ng = dr.num_groups as usize;
    if ng == 0 {
        return Ok(Vec::new());
    }

    let bpv = dr.bits_per_value as usize;
    let mut reader = BitReader::new(data);

    // 1. Read group reference values (each is bits_per_value bits)
    let mut group_refs = Vec::with_capacity(ng);
    for _ in 0..ng {
        group_refs.push(reader.read_bits(bpv) as i64);
    }
    reader.align_to_byte();

    // 2. Read group widths (each is group_width_bits bits)
    let gwb = dr.group_width_bits as usize;
    let mut group_widths = Vec::with_capacity(ng);
    for _ in 0..ng {
        group_widths.push(reader.read_bits(gwb) as usize + dr.group_width_ref as usize);
    }
    reader.align_to_byte();

    // 3. Read group lengths — read ALL ng values, then overwrite last with DRS value
    let glb = dr.group_length_bits as usize;
    let mut group_lengths = Vec::with_capacity(ng);
    for _ in 0..ng {
        let stored = reader.read_bits(glb) as usize;
        group_lengths
            .push(stored * dr.group_length_inc as usize + dr.group_length_ref as usize);
    }
    if ng > 0 {
        group_lengths[ng - 1] = dr.last_group_length as usize;
    }
    reader.align_to_byte();

    // 4. Unpack each group's values
    let total_values: usize = group_lengths.iter().sum();
    let mut raw = Vec::with_capacity(total_values);

    for g in 0..ng {
        let width = group_widths[g];
        let length = group_lengths[g];
        let gref = group_refs[g];

        for _ in 0..length {
            if width == 0 {
                raw.push(gref);
            } else {
                let val = reader.read_bits(width) as i64;
                raw.push(gref + val);
            }
        }
    }

    Ok(apply_scaling(&raw, dr))
}

/// Template 5.3: Complex packing with spatial differencing.
fn unpack_complex_spatial(data: &[u8], dr: &DataRepresentation) -> Result<Vec<f64>, String> {
    let order = dr.spatial_diff_order as usize;
    let extra_bytes = dr.spatial_diff_bytes as usize;

    if order == 0 || extra_bytes == 0 {
        return unpack_complex(data, dr);
    }

    let nbits = extra_bytes * 8;
    let mut reader = BitReader::new(data);

    // Read the initial values (1 for order=1, 2 for order=2)
    let mut initial_values = Vec::with_capacity(order);
    for _ in 0..order {
        let val = reader.read_bits(nbits) as i64;
        initial_values.push(val);
    }

    // Read the minimum value (sign-magnitude)
    let sign = reader.read_bits(1);
    let magnitude = reader.read_bits(nbits - 1) as i64;
    let minimum = if sign == 1 { -magnitude } else { magnitude };

    reader.align_to_byte();

    // Now read the rest as complex-packed groups from current position
    let consumed_bytes = (reader.bit_pos + 7) / 8;
    let remaining_data = &data[consumed_bytes..];

    let ng = dr.num_groups as usize;
    if ng == 0 {
        return Ok(Vec::new());
    }

    let bpv = dr.bits_per_value as usize;
    let mut greader = BitReader::new(remaining_data);

    // Read group references
    let mut group_refs = Vec::with_capacity(ng);
    for _ in 0..ng {
        group_refs.push(greader.read_bits(bpv) as i64);
    }
    greader.align_to_byte();

    // Read group widths
    let gwb = dr.group_width_bits as usize;
    let mut group_widths = Vec::with_capacity(ng);
    for _ in 0..ng {
        group_widths.push(greader.read_bits(gwb) as usize + dr.group_width_ref as usize);
    }
    greader.align_to_byte();

    // Read group lengths — must read ALL ng values from the stream (not ng-1),
    // then overwrite the last one with the true last group length from the DRS.
    // This matches g2clib behavior and ensures correct bit alignment for packed data.
    let glb = dr.group_length_bits as usize;
    let mut group_lengths = Vec::with_capacity(ng);
    for _ in 0..ng {
        let stored = greader.read_bits(glb) as usize;
        group_lengths
            .push(stored * dr.group_length_inc as usize + dr.group_length_ref as usize);
    }
    // Overwrite last group length with the true value from DRS
    if ng > 0 {
        group_lengths[ng - 1] = dr.last_group_length as usize;
    }
    greader.align_to_byte();

    // Unpack group values
    let total_values: usize = group_lengths.iter().sum();
    let mut raw = Vec::with_capacity(total_values);


    for g in 0..ng {
        let width = group_widths[g];
        let length = group_lengths[g];
        let gref = group_refs[g];

        for _ in 0..length {
            if width == 0 {
                raw.push(gref);
            } else {
                let val = greader.read_bits(width) as i64;
                raw.push(gref + val);
            }
        }
    }

    // Add minimum to all values
    for v in raw.iter_mut() {
        *v += minimum;
    }

    // Reconstruct from spatial differencing.
    // The complex-packed groups contain ALL n values (including positions 0..order).
    // Replace the first `order` values with the actual initial values read from the header.
    let mut reconstructed = raw;

    for (i, &iv) in initial_values.iter().enumerate() {
        if i < reconstructed.len() {
            reconstructed[i] = iv;
        }
    }

    if order == 1 {
        for i in 1..reconstructed.len() {
            reconstructed[i] += reconstructed[i - 1];
        }
    } else if order == 2 {
        for i in 2..reconstructed.len() {
            reconstructed[i] += 2 * reconstructed[i - 1] - reconstructed[i - 2];
        }
    }

    Ok(apply_scaling(&reconstructed, dr))
}

/// Template 5.40: JPEG2000 packing (stub for platforms without openjp2).
#[cfg(not(feature = "jpeg2000"))]
fn unpack_jpeg2000(_data: &[u8], _dr: &DataRepresentation) -> Result<Vec<f64>, String> {
    Err("JPEG2000 decoding not available (openjp2 feature disabled)".into())
}

/// Template 5.40: JPEG2000 packing.
///
/// Uses openjp2's C-style API to decode a JPEG2000 codestream embedded in GRIB2 Section 7.
/// GRIB2 JPEG2000 data is always a raw J2K codestream (starts with FF 4F).
#[cfg(feature = "jpeg2000")]
fn unpack_jpeg2000(data: &[u8], dr: &DataRepresentation) -> Result<Vec<f64>, String> {
    use openjp2::openjpeg::*;
    use std::ffi::c_void;

    if data.is_empty() {
        return Ok(Vec::new());
    }

    // Detect format - GRIB2 embeds raw J2K codestreams (0xFF 0x4F)
    let format = openjp2::detect_format(data)
        .map_err(|e| format!("JPEG2000 format detection failed: {}", e))?;
    let codec_format = match format {
        openjp2::J2KFormat::J2K => OPJ_CODEC_J2K,
        openjp2::J2KFormat::JP2 => OPJ_CODEC_JP2,
        openjp2::J2KFormat::JPT => OPJ_CODEC_JPT,
    };

    // We use the C-style FFI API because the Rust Stream type has no public
    // constructor for in-memory buffers.

    // WrappedSlice for the stream callbacks
    struct WrappedSlice {
        offset: usize,
        len: usize,
        ptr: *const u8,
    }

    extern "C" fn j2k_read(p_buffer: *mut c_void, nb_bytes: usize, p_data: *mut c_void) -> usize {
        if p_buffer.is_null() || nb_bytes == 0 {
            return usize::MAX;
        }
        let slice = unsafe { &mut *(p_data as *mut WrappedSlice) };
        let remaining = slice.len - slice.offset;
        if remaining == 0 {
            return usize::MAX;
        }
        let n = remaining.min(nb_bytes);
        unsafe {
            std::ptr::copy_nonoverlapping(slice.ptr.add(slice.offset), p_buffer as *mut u8, n);
        }
        slice.offset += n;
        n
    }

    extern "C" fn j2k_skip(nb_bytes: i64, p_data: *mut c_void) -> i64 {
        let slice = unsafe { &mut *(p_data as *mut WrappedSlice) };
        let new_off = (slice.offset as i64 + nb_bytes).max(0) as usize;
        slice.offset = new_off.min(slice.len);
        nb_bytes
    }

    extern "C" fn j2k_seek(nb_bytes: i64, p_data: *mut c_void) -> i32 {
        let slice = unsafe { &mut *(p_data as *mut WrappedSlice) };
        let off = nb_bytes as usize;
        if off <= slice.len {
            slice.offset = off;
            1
        } else {
            0
        }
    }

    extern "C" fn j2k_free(p_data: *mut c_void) {
        drop(unsafe { Box::from_raw(p_data as *mut WrappedSlice) });
    }

    let data_len = data.len();
    let wrapped = Box::new(WrappedSlice {
        offset: 0,
        len: data_len,
        ptr: data.as_ptr(),
    });
    let p_data = Box::into_raw(wrapped) as *mut c_void;

    // Create stream
    let stream = unsafe {
        let s = opj_stream_default_create(1);
        if s.is_null() {
            // Clean up wrapped data
            drop(Box::from_raw(p_data as *mut WrappedSlice));
            return Err("Failed to create JPEG2000 stream".into());
        }
        opj_stream_set_read_function(s, Some(j2k_read));
        opj_stream_set_skip_function(s, Some(j2k_skip));
        opj_stream_set_seek_function(s, Some(j2k_seek));
        opj_stream_set_user_data_length(s, data_len as u64);
        opj_stream_set_user_data(s, p_data, Some(j2k_free));
        s
    };

    // Create codec
    let codec = opj_create_decompress(codec_format);
    if codec.is_null() {
        unsafe { opj_stream_destroy(stream); }
        return Err("Failed to create JPEG2000 decoder".into());
    }

    // Setup decoder
    let mut params = opj_dparameters_t::default();
    let ret = unsafe { opj_setup_decoder(codec, &mut params) };
    if ret == 0 {
        unsafe {
            opj_destroy_codec(codec);
            opj_stream_destroy(stream);
        }
        return Err("Failed to setup JPEG2000 decoder".into());
    }

    // Read header
    let mut image: *mut opj_image_t = std::ptr::null_mut();
    let ret = unsafe { opj_read_header(stream, codec, &mut image) };
    if ret == 0 || image.is_null() {
        unsafe {
            opj_destroy_codec(codec);
            opj_stream_destroy(stream);
            if !image.is_null() {
                opj_image_destroy(image);
            }
        }
        return Err("Failed to read JPEG2000 header".into());
    }

    // Decode
    let ret = unsafe { opj_decode(codec, stream, image) };
    if ret == 0 {
        unsafe {
            opj_destroy_codec(codec);
            opj_stream_destroy(stream);
            opj_image_destroy(image);
        }
        return Err("JPEG2000 decode failed".into());
    }

    // End decompress
    unsafe { opj_end_decompress(codec, stream); }

    // Extract data from first component
    let img = unsafe { &*image };
    let numcomps = img.numcomps as usize;
    if numcomps == 0 || img.comps.is_null() {
        unsafe {
            opj_destroy_codec(codec);
            opj_stream_destroy(stream);
            opj_image_destroy(image);
        }
        return Err("JPEG2000 image has no components".into());
    }

    let comp = unsafe { &*img.comps };
    let n = (comp.w * comp.h) as usize;
    let raw: Vec<i64> = if let Some(data_slice) = comp.data() {
        data_slice.iter().take(n).map(|&v| v as i64).collect()
    } else {
        unsafe {
            opj_destroy_codec(codec);
            opj_stream_destroy(stream);
            opj_image_destroy(image);
        }
        return Err("JPEG2000 component has no data".into());
    };

    // Cleanup
    unsafe {
        opj_destroy_codec(codec);
        opj_stream_destroy(stream);
        opj_image_destroy(image);
    }

    Ok(apply_scaling(&raw, dr))
}

/// Template 5.41: PNG packing.
fn unpack_png(data: &[u8], dr: &DataRepresentation) -> Result<Vec<f64>, String> {
    if data.is_empty() {
        return Ok(Vec::new());
    }

    let decoder = png::Decoder::new(std::io::Cursor::new(data));
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("PNG decode error: {}", e))?;

    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("PNG frame error: {}", e))?;
    let bytes = &buf[..info.buffer_size()];

    let bpv = dr.bits_per_value as usize;
    let mut raw = Vec::new();

    match bpv {
        8 => {
            for &b in bytes {
                raw.push(b as i64);
            }
        }
        16 => {
            for chunk in bytes.chunks_exact(2) {
                raw.push(u16::from_be_bytes([chunk[0], chunk[1]]) as i64);
            }
        }
        24 => {
            for chunk in bytes.chunks_exact(3) {
                let v =
                    ((chunk[0] as u32) << 16) | ((chunk[1] as u32) << 8) | chunk[2] as u32;
                raw.push(v as i64);
            }
        }
        32 => {
            for chunk in bytes.chunks_exact(4) {
                raw.push(
                    u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]) as i64,
                );
            }
        }
        _ => {
            for &b in bytes {
                raw.push(b as i64);
            }
        }
    }

    Ok(apply_scaling(&raw, dr))
}
