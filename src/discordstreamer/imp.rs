use std::sync::atomic::{AtomicU16, Ordering};
use discortp::{Packet};
use gst::{Caps, error, FlowError, Fraction, glib, Pad, PadTemplate};
use gst::prelude::*;
use gst::subclass::prelude::*;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tokio::runtime::Handle;
use xsalsa20poly1305::aead::NewAead;
use xsalsa20poly1305::{KEY_SIZE, XSalsa20Poly1305 as Cipher};

use crate::constants::{RTP_AV1_PROFILE_TYPE, RTP_H264_PROFILE_TYPE, RTP_PACKET_MAX_SIZE, RTP_VERSION, RTP_VP8_PROFILE_TYPE, RTP_VP9_PROFILE_TYPE};
use crate::crypto::CryptoState;

pub static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new(
        "discordstreamer",
        gst::DebugColorFlags::empty(),
        Some(env!("CARGO_PKG_DESCRIPTION")),
    )
});

struct State {
    handle: Option<Handle>,
    crypto_state: CryptoState,
    cipher: Cipher,
}

struct Pads {
    video_sink: Pad,
    audio_sink: Option<Pad>,
}

pub struct DiscordStreamer {
    state: Mutex<State>,
    pads: Mutex<Pads>,

    video_sequence: AtomicU16,
    audio_sequence: AtomicU16,
}

impl DiscordStreamer {
    fn get_video_sequence(&self) -> u16 {
        let sequence = self.video_sequence.fetch_add(1, Ordering::Relaxed);
        if sequence == u16::MAX {
            self.video_sequence.store(0, Ordering::Relaxed);
        }
        sequence
    }

    fn get_audio_sequence(&self) -> u16 {
        let sequence = self.audio_sequence.fetch_add(1, Ordering::Relaxed);
        if sequence == u16::MAX {
            self.audio_sequence.store(0, Ordering::Relaxed);
        }
        sequence
    }

    pub fn set_tokio_runtime(
        &self,
        handle: Handle,
    ) {
        let _ = self.state.lock().handle.insert(handle);
    }

    fn runtime_handle(&self) -> Handle {
        self.state.lock().handle.as_ref().unwrap_or(crate::RUNTIME.handle()).clone()
    }

    //https://github.com/serenity-rs/songbird/blob/22fe3f3d4e43db67f1cdb7c9574867539517fb51/src/driver/tasks/mixer.rs#L484
    fn video_sink_chain(
        &self,
        pad: &Pad,
        buffer: gst::Buffer,
    ) -> Result<gst::FlowSuccess, FlowError> {
        let mut packet = [0u8; RTP_PACKET_MAX_SIZE];

        let _ = buffer.copy_to_slice(0, &mut packet);
        let mut rtp = discortp::rtp::MutableRtpPacket::new(&mut packet[..]).expect(
            "FATAL: Too few bytes in self.packet for RTP header."
        );

        rtp.set_version(RTP_VERSION);

        let caps = pad.current_caps().expect("No caps on pad");
        let caps = caps.structure(0).expect("No structure on caps");

        let encoding_name = caps.name().as_str();

        let rtp_type = match encoding_name {
            "video/x-av1" => RTP_AV1_PROFILE_TYPE,
            "video/x-h264" => RTP_H264_PROFILE_TYPE,
            "video/x-vp8" => RTP_VP8_PROFILE_TYPE,
            "video/x-vp9" => RTP_VP9_PROFILE_TYPE,
            _ => return Err(FlowError::NotSupported)
        };
        rtp.set_payload_type(rtp_type);

        rtp.set_sequence(self.get_video_sequence().into());

        let fps = caps.get::<Fraction>("framerate").expect("No framerate on caps");
        let fps = fps.numer() as u32 / fps.denom() as u32;
        rtp.set_timestamp((90_000/fps).into());

        let payload_size = rtp.payload().len();

        let mut state = self.state.lock();

        let final_payload_size = state.crypto_state.write_packet_nonce(&mut rtp, 16 + payload_size);

        state.crypto_state.kind().encrypt_in_place(&mut rtp, &state.cipher, final_payload_size).expect("Failed to encrypt packet");

        Ok(gst::FlowSuccess::Ok)
    }

    fn audio_sink_chain(
        &self,
        pad: &Pad,
        buffer: gst::Buffer,
    ) -> Result<gst::FlowSuccess, FlowError> {
        error!(CAT, "Audio not supported yet");
        Err(FlowError::NotSupported)
    }
}

#[glib::object_subclass]
impl ObjectSubclass for DiscordStreamer {
    const NAME: &'static str = "DiscordStreamer";
    type Type = super::DiscordStreamer;
    type ParentType = gst::Element;

    fn with_class(klass: &Self::Class) -> Self {
        let templ = klass.pad_template("video_sink").unwrap();
        let video_sink = Pad::builder_with_template(&templ, Some("video_sink"))
            .chain_function(|pad, parent, buffer| {
                DiscordStreamer::catch_panic_pad_function(
                    parent,
                    || Err(FlowError::Error),
                    |s| s.video_sink_chain(pad, buffer),
                )
            })
            .build();

        Self {
            state: Mutex::new(State {
                handle: None,
                crypto_state: CryptoState::Normal,
                cipher: Cipher::new_from_slice(&[0u8; KEY_SIZE]).unwrap(),
            }),
            pads: Mutex::new(Pads {
                video_sink,
                audio_sink: None,
            }),
            video_sequence: AtomicU16::new(0),
            audio_sequence: AtomicU16::new(0),
        }
    }
}

impl ObjectImpl for DiscordStreamer {
    fn constructed(&self) {
        self.parent_constructed();

        self.obj().add_pad(&self.pads.lock().video_sink).unwrap();
    }
}

impl GstObjectImpl for DiscordStreamer {}

impl ElementImpl for DiscordStreamer {
    fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
        static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
            gst::subclass::ElementMetadata::new(
                "DiscordStreamer",
                "Sink/Video/Audio",
                env!("CARGO_PKG_DESCRIPTION"),
                "Lorenzo Rizzotti <dev@dreaming.codes>",
            )
        });

        Some(&*ELEMENT_METADATA)
    }

    fn pad_templates() -> &'static [PadTemplate] {
        static PAD_TEMPLATES: Lazy<Vec<PadTemplate>> = Lazy::new(|| {
            let caps = Caps::builder_full()
                .structure(gst::Structure::builder("video/x-h264").field("stream-format", "byte-stream").field("profile", "baseline").build())
                .structure(gst::Structure::builder("video/x-vp8").build())
                .structure(gst::Structure::builder("video/x-vp9").build())
                .structure(gst::Structure::builder("video/x-av1").build())
                .build();

            let video_sink_pad_template = PadTemplate::new(
                "video_sink",
                gst::PadDirection::Sink,
                gst::PadPresence::Always,
                &caps,
            ).unwrap();

            let audio_sink_pad_template = PadTemplate::new(
                "audio_sink",
                gst::PadDirection::Sink,
                gst::PadPresence::Request,
                &Caps::new_empty_simple("audio/x-opus"),
            ).unwrap();

            vec![video_sink_pad_template, audio_sink_pad_template]
        });

        PAD_TEMPLATES.as_ref()
    }

    //TODO: Implement audio pad sink request
    fn request_new_pad(&self, templ: &PadTemplate, name: Option<&str>, _caps: Option<&Caps>) -> Option<Pad> {
        if templ.name_template() == "audio_sink" {
            let audio_sink = Pad::builder_with_template(templ, name)
                .chain_function(|pad, parent, buffer| {
                    DiscordStreamer::catch_panic_pad_function(
                        parent,
                        || Err(FlowError::Error),
                        |s| s.audio_sink_chain(pad, buffer),
                    )
                })
                .build();
            self.obj().add_pad(&audio_sink).unwrap();
            self.pads.lock().audio_sink = Some(audio_sink.clone());
            return Some(audio_sink);
        }

        None
    }
}