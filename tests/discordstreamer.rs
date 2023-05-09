use gst::prelude::*;
use gst::{debug_bin_to_dot_data, DebugGraphDetails};
use discordstreamer::discordstreamer::DiscordStreamer;

fn init() {
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        gst::init().unwrap();
        discordstreamer::plugin_register_static().unwrap();
    })
}

fn init_tests_dir() {
    use std::sync::Once;
    static INIT: Once = Once::new();

    INIT.call_once(|| {
        let _ = std::fs::remove_dir_all("./target/debug/tests");
        std::fs::create_dir_all("./target/debug/tests").expect("Failed to create tests dir");
    })
}

#[test]
fn pipeline_creation_test() {
    init();
    let pipeline = gst::Pipeline::new(None);

    let discord_streamer = DiscordStreamer::default();
    pipeline.add(&discord_streamer).expect("Failed to add discord_streamer to the pipeline");


    let video_test_src = gst::ElementFactory::make("videotestsrc").build().unwrap();
    pipeline.add(&video_test_src).expect("Failed to add video_test_src to the pipeline");

    let audio_test_src = gst::ElementFactory::make("audiotestsrc").build().unwrap();
    pipeline.add(&audio_test_src).expect("Failed to add audio_test_src to the pipeline");


    let video_convert = gst::ElementFactory::make("videoconvert").build().unwrap();
    pipeline.add(&video_convert).expect("Failed to add videoconvert to the pipeline");

    let audio_convert = gst::ElementFactory::make("audioconvert").build().unwrap();
    pipeline.add(&audio_convert).expect("Failed to add audioconvert to the pipeline");


    let h264_encoder = gst::ElementFactory::make("x264enc").build().unwrap();
    pipeline.add(&h264_encoder).expect("Failed to add x264enc to the pipeline");

    let opus_encoder = gst::ElementFactory::make("opusenc").build().unwrap();
    pipeline.add(&opus_encoder).expect("Failed to add opusenc to the pipeline");


    video_test_src.link(&video_convert).expect("Failed to link video_test_src and videoconvert");
    video_convert.link(&h264_encoder).expect("Failed to link videoconvert and x264enc");
    h264_encoder.link(&discord_streamer).expect("Failed to link videoconvert and discord_streamer");

    audio_test_src.link(&audio_convert).expect("Failed to link audio_test_src and audioconvert");
    audio_convert.link(&opus_encoder).expect("Failed to link audioconvert and opusenc");
    opus_encoder.link(&discord_streamer).expect("Failed to link audioconvert and discord_streamer");

    pipeline.set_state(gst::State::Playing).expect("Failed to set pipeline state");

    // Debug diagram
    let out = debug_bin_to_dot_data(&pipeline, DebugGraphDetails::ALL);
    init_tests_dir();
    std::fs::write(
        "./target/debug/tests/pipeline.dot",
        out.as_str(),
    )
        .unwrap();
}