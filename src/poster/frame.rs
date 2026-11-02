use image::RgbaImage;
use openh264::decoder::Decoder;

pub const POSTER_WIDTH: u32 = 640;

pub fn annexb_nals(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push((i, i + 3));
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::new();
    for (n, (_, payload)) in starts.iter().enumerate() {
        let mut end = starts.get(n + 1).map_or(data.len(), |(s, _)| *s);
        while end > *payload && data[end - 1] == 0 {
            end -= 1;
        }
        if end > *payload {
            out.push(&data[*payload..end]);
        }
    }
    out
}

fn find_box<'a>(data: &'a [u8], name: &[u8; 4]) -> Option<&'a [u8]> {
    let mut i = 0;
    while i + 8 <= data.len() {
        let size = u32::from_be_bytes(data[i..i + 4].try_into().ok()?) as usize;
        let size = if size == 0 { data.len() - i } else { size };
        if size < 8 || i + size > data.len() {
            return None;
        }
        if &data[i + 4..i + 8] == name {
            return Some(&data[i + 8..i + size]);
        }
        i += size;
    }
    None
}

fn find_nested<'a>(data: &'a [u8], path: &[&[u8; 4]]) -> Option<&'a [u8]> {
    let mut cur = data;
    for name in path {
        cur = find_box(cur, name)?;
    }
    Some(cur)
}

pub struct AvcConfig {
    pub nal_length_size: usize,
    pub parameter_sets: Vec<Vec<u8>>,
}

pub fn parse_avcc(init: &[u8]) -> Option<AvcConfig> {
    let stsd = find_nested(init, &[b"moov", b"trak", b"mdia", b"minf", b"stbl", b"stsd"])?;
    let entry = stsd.get(8..)?;
    let visual = entry.get(8 + 78..)?;
    let avcc = find_box(visual, b"avcC")?;
    if avcc.len() < 7 {
        return None;
    }
    let nal_length_size = (avcc[4] & 3) as usize + 1;
    let mut sets = Vec::new();
    let mut p = 6;
    let sps_count = (avcc[5] & 0x1f) as usize;
    for _ in 0..sps_count {
        let len = u16::from_be_bytes(avcc.get(p..p + 2)?.try_into().ok()?) as usize;
        sets.push(avcc.get(p + 2..p + 2 + len)?.to_vec());
        p += 2 + len;
    }
    let pps_count = *avcc.get(p)? as usize;
    p += 1;
    for _ in 0..pps_count {
        let len = u16::from_be_bytes(avcc.get(p..p + 2)?.try_into().ok()?) as usize;
        sets.push(avcc.get(p + 2..p + 2 + len)?.to_vec());
        p += 2 + len;
    }
    Some(AvcConfig {
        nal_length_size,
        parameter_sets: sets,
    })
}

pub fn fmp4_to_annexb(init: &[u8], segment: &[u8]) -> Option<Vec<u8>> {
    let cfg = parse_avcc(init)?;
    let mdat = find_box(segment, b"mdat")?;
    let mut out = Vec::new();
    for s in &cfg.parameter_sets {
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(s);
    }
    let mut p = 0;
    while p + cfg.nal_length_size <= mdat.len() {
        let mut len = 0usize;
        for b in &mdat[p..p + cfg.nal_length_size] {
            len = (len << 8) | *b as usize;
        }
        p += cfg.nal_length_size;
        if len == 0 || p + len > mdat.len() {
            break;
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&mdat[p..p + len]);
        p += len;
    }
    Some(out)
}

fn ts_payload(pkt: &[u8]) -> Option<(u16, bool, &[u8])> {
    if pkt.len() != 188 || pkt[0] != 0x47 {
        return None;
    }
    let pusi = pkt[1] & 0x40 != 0;
    let pid = (((pkt[1] & 0x1f) as u16) << 8) | pkt[2] as u16;
    let afc = (pkt[3] >> 4) & 3;
    let mut start = 4;
    if afc & 2 != 0 {
        start += 1 + *pkt.get(4)? as usize;
    }
    if afc & 1 == 0 || start >= 188 {
        return None;
    }
    Some((pid, pusi, &pkt[start..]))
}

fn find_video_pid(data: &[u8]) -> Option<u16> {
    let mut pmt_pids = Vec::new();
    for pkt in data.chunks_exact(188) {
        let Some((pid, pusi, payload)) = ts_payload(pkt) else {
            continue;
        };
        if pid == 0 && pusi {
            let sec = payload.get(1 + payload[0] as usize..)?;
            let section_len = (((sec.get(1)? & 0x0f) as usize) << 8) | *sec.get(2)? as usize;
            let mut p = 8;
            while p + 4 <= (3 + section_len).saturating_sub(4).min(sec.len()) {
                let program = u16::from_be_bytes([sec[p], sec[p + 1]]);
                let pid = (((sec[p + 2] & 0x1f) as u16) << 8) | sec[p + 3] as u16;
                if program != 0 {
                    pmt_pids.push(pid);
                }
                p += 4;
            }
        } else if pusi && pmt_pids.contains(&pid) {
            let sec = payload.get(1 + payload[0] as usize..)?;
            let section_len = (((sec.get(1)? & 0x0f) as usize) << 8) | *sec.get(2)? as usize;
            let info_len = (((sec.get(10)? & 0x0f) as usize) << 8) | *sec.get(11)? as usize;
            let mut p = 12 + info_len;
            let end = (3 + section_len).saturating_sub(4).min(sec.len());
            while p + 5 <= end {
                let stream_type = sec[p];
                let es_pid = (((sec[p + 1] & 0x1f) as u16) << 8) | sec[p + 2] as u16;
                let es_info = (((sec[p + 3] & 0x0f) as usize) << 8) | sec[p + 4] as usize;
                if stream_type == 0x1b {
                    return Some(es_pid);
                }
                p += 5 + es_info;
            }
        }
    }
    None
}

pub fn ts_to_annexb(data: &[u8]) -> Option<Vec<u8>> {
    let pid = find_video_pid(data)?;
    let mut out = Vec::new();
    for pkt in data.chunks_exact(188) {
        let Some((p, pusi, payload)) = ts_payload(pkt) else {
            continue;
        };
        if p != pid {
            continue;
        }
        if pusi {
            if payload.len() < 9 || payload[0..3] != [0, 0, 1] {
                continue;
            }
            let header = 9 + payload[8] as usize;
            if let Some(es) = payload.get(header..) {
                out.extend_from_slice(es);
            }
        } else {
            out.extend_from_slice(payload);
        }
    }
    Some(out)
}

pub fn decode_first_frame(annexb: &[u8]) -> Option<RgbaImage> {
    use openh264::formats::YUVSource;
    let mut decoder = Decoder::new().ok()?;
    for nal in annexb_nals(annexb) {
        let mut packet = Vec::with_capacity(nal.len() + 4);
        packet.extend_from_slice(&[0, 0, 0, 1]);
        packet.extend_from_slice(nal);
        if let Ok(Some(yuv)) = decoder.decode(&packet) {
            let (w, h) = yuv.dimensions();
            let mut rgba = vec![0u8; w * h * 4];
            yuv.write_rgba8(&mut rgba);
            return RgbaImage::from_raw(w as u32, h as u32, rgba);
        }
    }
    None
}

pub fn downscale(img: RgbaImage, width: u32) -> RgbaImage {
    if img.width() <= width {
        return img;
    }
    let height = (img.height() as u64 * width as u64 / img.width() as u64).max(1) as u32;
    image::imageops::resize(&img, width, height, image::imageops::FilterType::Triangle)
}

pub fn frame_from_fmp4(init: &[u8], segment: &[u8]) -> Option<RgbaImage> {
    decode_first_frame(&fmp4_to_annexb(init, segment)?).map(|i| downscale(i, POSTER_WIDTH))
}

pub fn frame_from_ts(segment: &[u8]) -> Option<RgbaImage> {
    decode_first_frame(&ts_to_annexb(segment)?).map(|i| downscale(i, POSTER_WIDTH))
}

#[cfg(test)]
mod tests {
    use super::*;

    const INIT: &[u8] = include_bytes!("testdata/init.mp4");
    const SEG: &[u8] = include_bytes!("testdata/seg0.m4s");
    const TS: &[u8] = include_bytes!("testdata/t.ts");

    #[test]
    fn decodes_first_keyframe_of_fmp4_segment() {
        let img = frame_from_fmp4(INIT, SEG).expect("frame");
        assert_eq!((img.width(), img.height()), (160, 96));
        assert!(img.pixels().any(|p| p.0[0] != p.0[2]));
    }

    #[test]
    fn decodes_first_keyframe_of_mpegts_segment() {
        let img = frame_from_ts(TS).expect("frame");
        assert_eq!((img.width(), img.height()), (160, 96));
    }

    #[test]
    fn garbage_gives_no_frame() {
        assert!(frame_from_fmp4(&[1, 2, 3], &[4, 5, 6]).is_none());
        assert!(frame_from_ts(&[0u8; 188]).is_none());
    }

    #[test]
    fn downscale_keeps_aspect_and_skips_small() {
        let big = RgbaImage::new(1920, 1080);
        let s = downscale(big, 640);
        assert_eq!((s.width(), s.height()), (640, 360));
        let small = RgbaImage::new(100, 50);
        assert_eq!(downscale(small, 640).width(), 100);
    }

    #[test]
    fn splits_annexb_nals() {
        let d = [0, 0, 0, 1, 0x67, 1, 2, 0, 0, 1, 0x68, 3];
        let n = annexb_nals(&d);
        assert_eq!(n, vec![&[0x67, 1, 2][..], &[0x68, 3][..]]);
    }
}
