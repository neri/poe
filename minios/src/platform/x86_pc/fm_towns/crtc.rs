//! FM TOWNS Video Control I/O
use x86::isolated_io::*;

const CRTC_INDEX: IoPortWB = IoPortWB(0x440);
const CRTC_DATA: IoPortWW = IoPortWW(0x442);

const VIDEO_OUT_CTL_INDEX: IoPortWB = IoPortWB(0x448);
const VIDEO_OUT_CTL_DATA: IoPortWB = IoPortWB(0x44a);

pub struct Crtc;

impl Crtc {
    #[inline]
    pub unsafe fn crtc_out(index: u8, data: u16) {
        unsafe {
            CRTC_INDEX.write(index);
            CRTC_DATA.write(data);
        }
    }

    #[inline]
    pub unsafe fn video_output_control(index: u8, data: u8) {
        unsafe {
            VIDEO_OUT_CTL_INDEX.write(index);
            VIDEO_OUT_CTL_DATA.write(data);
        }
    }

    #[inline]
    pub unsafe fn set_mode(crt_desc: &[u16; 30], pmode_ctl: u8, priority: u8, output_ctl: u8) {
        unsafe {
            let mut iter = crt_desc.iter();
            Self::crtc_out(0, *iter.next().unwrap());
            Self::crtc_out(1, *iter.next().unwrap());

            for (index, data) in (4..32).zip(iter) {
                Self::crtc_out(index, *data);
            }

            Crtc::video_output_control(0, pmode_ctl);
            Crtc::video_output_control(1, priority);
            IoPortWB(0xfda0).write(output_ctl);
        }
    }
}
