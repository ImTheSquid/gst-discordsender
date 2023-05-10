use discortp::rtp::RtpType;

pub const RTP_VERSION: u8 = 2;
//TODO: Figure out the right value
pub const RTP_PACKET_MAX_SIZE: usize = 1460;
pub const RTP_OPUS_PROFILE_TYPE: RtpType = RtpType::Dynamic(120);
pub const RTP_AV1_PROFILE_TYPE: RtpType = RtpType::Dynamic(101);
pub const RTP_AV1_RTX_PROFILE_TYPE: RtpType = RtpType::Dynamic(102);
pub const RTP_H264_PROFILE_TYPE: RtpType = RtpType::Dynamic(103);
pub const RTP_H264_RTX_PROFILE_TYPE: RtpType = RtpType::Dynamic(104);
pub const RTP_VP8_PROFILE_TYPE: RtpType = RtpType::Dynamic(105);
pub const RTP_VP8_RTX_PROFILE_TYPE: RtpType = RtpType::Dynamic(106);
pub const RTP_VP9_PROFILE_TYPE: RtpType = RtpType::Dynamic(107);
pub const RTP_VP9_RTX_PROFILE_TYPE: RtpType = RtpType::Dynamic(108);