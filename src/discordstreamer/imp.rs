use gst::{Caps, glib, info, Pad, PadTemplate};
use gst::prelude::*;
use gst::subclass::prelude::*;
use once_cell::sync::Lazy;
use parking_lot::Mutex;
use tokio::runtime::Handle;

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
}

#[glib::object_subclass]
impl ObjectSubclass for DiscordStreamer {
    const NAME: &'static str = "DiscordStreamer";
    type Type = super::DiscordStreamer;
    type ParentType = gst::Element;

    fn with_class(klass: &Self::Class) -> Self {
        let templ = klass.pad_template("video_sink").unwrap();
        let video_sink = Pad::builder_with_template(&templ, Some("video_sink")).build();

        Self {
            state: Mutex::new(State{
                handle: None,
                video_sink,
                audio_sink: None,
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