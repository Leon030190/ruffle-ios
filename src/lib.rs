use objc2::runtime::AnyClass;
use objc2::ClassType;
use objc2_foundation::{MainThreadMarker, NSString};
use objc2_ui_kit::UIApplication;
use tracing_subscriber::fmt::Layer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

mod add_controller;
mod app_delegate;
mod edit_controller;
mod library_controller;
mod player_controller;
mod player_view;
mod scene_delegate;
mod storage;
pub mod virtual_gamepad;
pub mod web_downloader;

pub use self::app_delegate::AppDelegate;
pub use self::player_controller::PlayerController;
pub use self::player_view::PlayerView;
pub use self::virtual_gamepad::VirtualGamepadView;

/// Emit logging to either OSLog or stderr, depending on if using Mac
/// Catalyst or native.
///
/// TODO: If running Mac Catalyst under Xcode
pub fn init_logging() {
    let subscriber = tracing_subscriber::registry();

    let env_filter = tracing_subscriber::EnvFilter::builder()
        .parse_lossy(std::env::var("RUST_LOG").as_deref().unwrap_or("info"));

    let subscriber = subscriber.with(env_filter);

    #[cfg(target_abi = "macabi")]
    let subscriber = subscriber.with(Layer::new().with_writer(std::io::stderr));

    #[cfg(not(target_abi = "macabi"))]
    let subscriber = subscriber.with(tracing_oslog::OsLogger::default());

    subscriber.init();
}

pub fn launch(app_class: Option<&AnyClass>, delegate_class: Option<&AnyClass>) {
    // Set inside Info.plist
    let _ = scene_delegate::SceneDelegate::class();

    // These classes are loaded from a storyboard,
    // and hence need to be initialized first.
    // See also [storyboard_connections.h]
    let _ = player_view::PlayerView::class();
    let _ = virtual_gamepad::VirtualGamepadView::class();
    let _ = player_controller::PlayerController::class();
    let _ = library_controller::LibraryController::class();
    let _ = edit_controller::EditController::class();
    let _ = add_controller::AddController::class();

    // This is loaded by CoreData
    let _ = storage::Movie::class();
    let _ = storage::MovieData::class();

    let mtm = MainThreadMarker::new().unwrap();
    UIApplication::main(
        app_class.map(|cls| NSString::from_class(cls)).as_deref(),
        delegate_class
            .map(|cls| NSString::from_class(cls))
            .as_deref(),
        mtm,
    );
}
