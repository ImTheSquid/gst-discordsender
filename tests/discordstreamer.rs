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

    pipeline
        .add(&discord_streamer)
        .expect("Failed to add discord_streamer to the pipeline");

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