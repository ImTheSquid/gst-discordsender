use std::net::UdpSocket;
use std::sync::atomic::{AtomicU16, Ordering};
use discortp::{Packet};
use gst::{Caps, debug, error, FlowError, Fraction, glib, Pad, PadTemplate};
use gst::glib::{ParamSpec, Value};
use gst::prelude::*;
use gst::subclass::prelude::*;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use xsalsa20poly1305::{KeyInit, TAG_SIZE};
use xsalsa20poly1305::{Key, KEY_SIZE, XSalsa20Poly1305 as Cipher};

use crate::constants::{RTP_AV1_PROFILE_TYPE, RTP_H264_PROFILE_TYPE, RTP_VERSION, RTP_VP8_PROFILE_TYPE, RTP_VP9_PROFILE_TYPE};
use crate::crypto::{CryptoMode, CryptoState};

pub static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
    gst::DebugCategory::new(
        "discordstreamer",
        gst::DebugColorFlags::empty(),
        Some(env!("CARGO_PKG_DESCRIPTION")),
    )
});

struct State {
    crypto_state: CryptoState,
    cipher: Cipher,
    udp_socket: UdpSocket,
    video_ssrc: u32,
    audio_ssrc: u32
}

impl State {
    fn from_props(props: &Props) -> Result<Self, gst::ErrorMessage> {
        let crypto_state = CryptoState::from(serde_plain::from_str::<CryptoMode>(props.crypto_mode.as_str()).map_err(|e| {
            gst::error_msg!(
                gst::ResourceError::Failed,
                ["Failed to parse crypto mode: {}", e]
            )
        })?);

        let Some(crypto_key) = &props.crypto_key else {
            return Err(gst::error_msg!(
                gst::ResourceError::NotFound,
                ["No crypto key provided"]
            ));
        };

        if crypto_key.len() != KEY_SIZE {
            return Err(gst::error_msg!(
                gst::ResourceError::Failed,
                ["Crypto key must be {} bytes long", KEY_SIZE]
            ));
        }

        let mut key = [0u8; KEY_SIZE];
        key.copy_from_slice(crypto_key);
        let key = Key::from(key);

        let cipher = Cipher::new(&key);

        let Some(address) = &props.address else {
            return Err(gst::error_msg!(
                gst::ResourceError::NotFound,
                ["No address provided"]
            ));
        };

        let Ok(udp_socket) = UdpSocket::bind("0.0.0.0:0") else {
            return Err(gst::error_msg!(
                gst::ResourceError::Failed,
                ["Failed to bind UDP socket"]
            ));
        };

        if let Err(error) = udp_socket.connect(address.as_str()) {
            return Err(gst::error_msg!(
                gst::ResourceError::Failed,
                ["Failed to connect UDP socket to {}: {}", address, error]
            ));
        };

        let Some(video_ssrc) = props.video_ssrc else {
            return Err(gst::error_msg!(
                gst::ResourceError::NotFound,
                ["No video SSRC provided"]
            ));
        };

        let Some(audio_ssrc) = props.audio_ssrc else {
            return Err(gst::error_msg!(
                gst::ResourceError::NotFound,
                ["No audio SSRC provided"]
            ));
        };

        Ok(Self {
            crypto_state,
            cipher,
            udp_socket,
            video_ssrc,
            audio_ssrc
        })
    }
}

struct Pads {
    video_sink: Pad,
    audio_sink: Option<Pad>,
}

struct Props {
    crypto_key: Option<glib::Bytes>,
    crypto_mode: glib::GString,
    address: Option<glib::GString>,
    video_ssrc: Option<u32>,
    audio_ssrc: Option<u32>,
}

impl Default for Props {
    fn default() -> Self {
        Self {
            crypto_key: None,
            crypto_mode: serde_plain::to_string(&CryptoMode::Lite).unwrap().into(),
            address: None,
            video_ssrc: None,
            audio_ssrc: None,
        }
    }
}

pub struct DiscordStreamer {
    state: Mutex<Option<State>>,
    pads: Mutex<Pads>,
    props: Mutex<Props>,

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

    //https://github.com/serenity-rs/songbird/blob/22fe3f3d4e43db67f1cdb7c9574867539517fb51/src/driver/tasks/mixer.rs#L484
    fn video_sink_chain(
        &self,
        pad: &Pad,
        buffer: gst::Buffer,
    ) -> Result<gst::FlowSuccess, FlowError> {
        let mut packet = vec![0u8; buffer.size()];

        let _ = buffer.copy_to_slice(0, &mut packet);
        let mut rtp = discortp::rtp::MutableRtpPacket::new(&mut packet[..]).expect(
            "FATAL: Too few bytes in self.packet for RTP header."
        );

        rtp.set_version(RTP_VERSION);

        let mut state = self.state.lock();
        let state = state.as_mut().expect("State not initialized");

        rtp.set_ssrc(state.video_ssrc);

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
        rtp.set_timestamp((90_000 / fps).into());

        let payload_size = rtp.payload().len() - state.crypto_state.kind().payload_suffix_len();

        //let final_payload_size = state.crypto_state.write_packet_nonce(&mut rtp, TAG_SIZE + payload_size);

        let final_payload_size = state.crypto_state.write_packet_nonce(&mut rtp,  payload_size);

        state.crypto_state.kind().encrypt_in_place(&mut rtp, &state.cipher, final_payload_size).expect("Failed to encrypt packet");

        let _ = state.udp_socket.send(&rtp.packet()[..final_payload_size]);

        Ok(gst::FlowSuccess::Ok)
    }

    fn audio_sink_chain(
        &self,
        _pad: &Pad,
        _buffer: gst::Buffer,
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
        let video_sink = Pad::builder_with_template(&templ, Some("video_sink")).chain_function(|pad, parent, buffer| {
            DiscordStreamer::catch_panic_pad_function(
                parent,
                || Err(FlowError::Error),
                |s| s.video_sink_chain(pad, buffer),
            )
        }).build();

        Self {
            state: Mutex::new(None),
            pads: Mutex::new(Pads {
                video_sink,
                audio_sink: None,
            }),
            props: Mutex::new(Default::default()),
            video_sequence: AtomicU16::new(0),
            audio_sequence: AtomicU16::new(0),
        }
    }
}

impl ObjectImpl for DiscordStreamer {
    fn properties() -> &'static [ParamSpec] {
        static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
            vec![
                glib::ParamSpecBoxed::builder::<glib::Bytes>("crypto-key").nick("Crypto Key").blurb("The key used to encrypt the stream").build(),
                glib::ParamSpecString::builder("crypto-mode").nick("Crypto Mode").blurb(
                    format!(
                        "The mode used to encrypt the stream. Available modes: {}, {}, {}",
                        serde_plain::to_string(&CryptoMode::Normal).unwrap(),
                        serde_plain::to_string(&CryptoMode::Lite).unwrap(),
                        serde_plain::to_string(&CryptoMode::Suffix).unwrap()).as_str()
                ).write_only().build(),
                glib::ParamSpecString::builder("address").nick("Address").blurb("The address to stream to").build(),
                glib::ParamSpecUInt::builder("video-ssrc").nick("Video ssrc").blurb("The ssrc to use for the rtp video packets").build(),
                glib::ParamSpecUInt::builder("audio-ssrc").nick("Audio ssrc").blurb("The ssrc to use for the rtp audio packets").build(),
            ]
        });

        PROPERTIES.as_ref()
    }


    fn set_property(&self, _id: usize, value: &Value, pspec: &ParamSpec) {
        match pspec.name() {
            "crypto-key" => {
                let mut props = self.props.lock();
                props.crypto_key = value.get().expect("type checked upstream");
            }

            "crypto-mode" => {
                let mut props = self.props.lock();
                props.crypto_mode = value.get().expect("type checked upstream");
            }

            "address" => {
                let mut props = self.props.lock();
                props.address = value.get().expect("type checked upstream");
            }

            "video-ssrc" => {
                let mut props = self.props.lock();
                props.video_ssrc = Some(value.get().expect("type checked upstream"));
            }

            "audio-ssrc" => {
                let mut props = self.props.lock();
                props.audio_ssrc = Some(value.get().expect("type checked upstream"));
            }

            _ => unimplemented!(),
        }
    }

    fn property(&self, _id: usize, pspec: &ParamSpec) -> Value {
        match pspec.name() {
            "crypto-key" => self.props.lock().crypto_key.to_value(),
            "crypto-mode" => self.props.lock().crypto_mode.to_value(),
            "address" => self.props.lock().address.to_value(),
            "video-ssrc" => self.props.lock().video_ssrc.map_or((None as Option<glib::GString>).to_value(), |v| v.to_value()),
            "audio-ssrc" => self.props.lock().audio_ssrc.map_or((None as Option<glib::GString>).to_value(), |v| v.to_value()),
            _ => unimplemented!(),
        }
    }

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
            let caps = Caps::builder_full().structure(gst::Structure::builder("video/x-h264").field("stream-format", "byte-stream").field("profile", "baseline").build()).structure(gst::Structure::builder("video/x-vp8").build()).structure(gst::Structure::builder("video/x-vp9").build()).structure(gst::Structure::builder("video/x-av1").build()).build();

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

    fn change_state(&self, transition: gst::StateChange) -> Result<gst::StateChangeSuccess, gst::StateChangeError> {
        debug!(CAT, imp: self, "Changing state {:?}", transition);

        match transition {
            gst::StateChange::NullToReady => {
                let props = self.props.lock();

                // Create an internal state struct from the provided properties or
                // refuse to change state
                let state_ = State::from_props(&props).map_err(|err| {
                    self.post_error_message(err);
                    gst::StateChangeError
                })?;

                let _ = self.state.lock().insert(state_);
            }
            gst::StateChange::ReadyToNull => {
                let _ = self.state.lock().take();
            }
            _ => (),
        }

        let success = self.parent_change_state(transition)?;

        if transition == gst::StateChange::ReadyToNull {
            let _ = self.state.lock().take();
        }

        Ok(success)
    }

    //TODO: Implement audio pad sink request
    fn request_new_pad(&self, templ: &PadTemplate, name: Option<&str>, _caps: Option<&Caps>) -> Option<Pad> {
        if templ.name_template() == "audio_sink" {
            let audio_sink = Pad::builder_with_template(templ, name).chain_function(|pad, parent, buffer| {
                DiscordStreamer::catch_panic_pad_function(
                    parent,
                    || Err(FlowError::Error),
                    |s| s.audio_sink_chain(pad, buffer),
                )
            }).build();
            self.obj().add_pad(&audio_sink).unwrap();
            self.pads.lock().audio_sink = Some(audio_sink.clone());
            return Some(audio_sink);
        }

        None
    }
}