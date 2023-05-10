use discortp::{MutablePacket, Packet};
use gst::{Caps, glib, info, Pad, PadTemplate};
use gst::prelude::*;
use gst::subclass::prelude::*;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use discortp::rtp::RtpType;
use tokio::runtime::Handle;
use xsalsa20poly1305::XSalsa20Poly1305 as Cipher;
use xsalsa20poly1305::aead::NewAead;
use crate::constants::RTP_VERSION;

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
    video_sink: Pad,
    audio_sink: Option<Pad>,
    crypto_state: CryptoState
}

pub struct DiscordStreamer {
    state: Mutex<State>,
}

impl DiscordStreamer {
    pub fn set_tokio_runtime(
        &self,
        handle: Handle
    ) {
        let _ = self.state.lock().handle.insert(handle);
    }

    fn runtime_handle(&self) -> Handle {
        self.state.lock().handle.as_ref().unwrap_or(crate::RUNTIME.handle()).clone()
    }

    //https://github.com/serenity-rs/songbird/blob/22fe3f3d4e43db67f1cdb7c9574867539517fb51/src/driver/tasks/mixer.rs#L484
    fn video_sink_chain(
        &self,
        pad: &gst::Pad,
        buffer: gst::Buffer,
    ) -> Result<gst::FlowSuccess, gst::FlowError> {
        //TODO: Figure out the right size for the packet
        let mut packet = [0u8; 1460];

        let _ = buffer.copy_to_slice(0, &mut packet);
        let mut rtp = discortp::rtp::MutableRtpPacket::new(&mut packet[..]).expect(
            "FATAL: Too few bytes in self.packet for RTP header."
        );

        rtp.set_version(RTP_VERSION);
        //TODO: Set this based on the codec
        rtp.set_payload_type(RtpType::Dynamic(111));
        //TODO: This should be incremented by 1 every packet (separate for audio and video)
        rtp.set_sequence(0.into());
        //TODO: Not sure about this one https://github.com/aiko-chan-ai/Discord-video-selfbot/blob/f14ea0a259e4bbf9ae995ec16f45ad767b3ebf39/src/Packet/AudioPacketizer.js#LL17C5-L17C5
        rtp.set_timestamp(0.into());

        let payload_size = rtp.payload().len();
        
        let final_payload_size = self.state.lock().crypto_state.write_packet_nonce(&mut rtp, 16 + payload_size);

        //TODO: Use real key and store Cipher in state
        self.state.lock().crypto_state.kind().encrypt_in_place(&mut rtp, &Cipher::new_from_slice(&[0u8; 4]).unwrap(), final_payload_size).expect("Failed to encrypt packet");


        Ok(gst::FlowSuccess::Ok)
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
                    || Err(gst::FlowError::Error),
                    |s| s.video_sink_chain(pad, buffer),
                )
            })
            .build();

        Self {
            state: Mutex::new(State{
                handle: None,
                video_sink,
                audio_sink: None,
                crypto_state: CryptoState::Normal
            }),
        }
    }

}
impl ObjectImpl for DiscordStreamer {
    fn constructed(&self) {
        self.parent_constructed();

        self.obj().add_pad(&self.state.lock().video_sink).unwrap();
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
            let caps = gst::Caps::builder_full()
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
    fn request_new_pad(&self, templ: &PadTemplate, name: Option<&str>, caps: Option<&Caps>) -> Option<Pad> {
        if templ.name_template() == "audio_sink" {
            let audio_sink = Pad::builder_with_template(templ, name).build();
            self.obj().add_pad(&audio_sink).unwrap();
            self.state.lock().audio_sink = Some(audio_sink.clone());
            return Some(audio_sink);
        }

        None
    }
}