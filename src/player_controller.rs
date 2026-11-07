use std::cell::{Cell, OnceCell};
use std::fs::File;
use std::path::Path;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;
use std::{fmt, io};

use block2::RcBlock;
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{define_class, msg_send, sel, DefinedClass as _, MainThreadOnly, Message};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSBundle, NSCoder, NSObjectProtocol, NSRunLoop, NSString,
};
use objc2_ui_kit::UIViewController;
use ruffle_core::backend::navigator::OwnedFuture;
use ruffle_core::backend::storage::StorageBackend;
use ruffle_core::config::Letterbox;
use ruffle_core::{LoadBehavior, Player, PlayerBuilder};
use ruffle_frontend_utils::backends::audio::CpalAudioBackend;
use ruffle_frontend_utils::backends::navigator::{
    self, ExternalNavigatorBackend, NavigatorInterface,
};
use ruffle_frontend_utils::content::PlayingContent;
use ruffle_frontend_utils::player_options::PlayerOptions;
use ruffle_render::quality::StageQuality;
use url::Url;

use crate::player_view::PlayerView;
use crate::storage::{self, Movie, SecurityScopedResource};
use crate::virtual_gamepad::VirtualGamepadView;

#[derive(Clone, Debug)]
pub struct FutureSpawner {
    mtm: MainThreadMarker,
    main_run_loop: Retained<NSRunLoop>,
}

impl FutureSpawner {
    fn run_later(&self, closure: impl FnOnce() + 'static) {
        let cell = Cell::new(Some(closure));

        let _ = self.mtm;
        // SAFTY: We hold MainThreadMarker, so it's fine to send a non-send
        // closures to be run later on the main thread.
        unsafe {
            self.main_run_loop.performBlock(&RcBlock::new(move || {
                let closure = cell.take().expect("called twice");
                closure();
            }))
        };
    }
}

impl<E: std::error::Error + 'static> navigator::FutureSpawner<E> for FutureSpawner {
    fn spawn(&self, future: OwnedFuture<(), E>) {
        // Discard any errors.
        let future = async {
            if let Err(e) = future.await {
                tracing::error!("Async error: {}", e);
            }
        };

        let scheduler = move |task: async_task::Runnable| {
            self.run_later(|| {
                task.run();
            });
        };

        // SAFETY: TODO
        let (runnable, task) = unsafe { async_task::spawn_unchecked(future, scheduler) };

        // The future should run in the background.
        task.detach();
        // Immediately schedule the future to be polled for the first time.
        runnable.schedule();
    }
}

#[derive(Default)]
pub struct Ivars {
    // Populated to be used in `viewDidLoad`.
    content: Cell<Option<PlayingContent>>,
    user_options: Cell<Option<PlayerOptions>>,
    storage_backend: Cell<Option<Box<dyn StorageBackend>>>,

    /// Used to keep the bundle resource alive while we're using it.
    _scoped_resource: Cell<Option<SecurityScopedResource>>,

    player: OnceCell<Arc<Mutex<Player>>>,
    gamepad: OnceCell<Retained<VirtualGamepadView>>,
}

impl fmt::Debug for Ivars {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ivars").finish_non_exhaustive()
    }
}

#[derive(Clone)]
struct Navigator;

impl NavigatorInterface for Navigator {
    fn navigate_to_website(&self, _url: Url) {}

    async fn open_file(&self, path: &Path) -> io::Result<File> {
        tracing::info!("trying to open: {path:?}");
        File::open(path)
    }

    async fn confirm_socket(&self, _host: &str, _port: u16) -> bool {
        true
    }
}

define_class!(
    #[unsafe(super(UIViewController))]
    #[name = "PlayerController"]
    #[ivars = Ivars]
    #[derive(Debug)]
    pub struct PlayerController;

    unsafe impl NSObjectProtocol for PlayerController {}

    /// UIViewController.
    impl PlayerController {
        #[unsafe(method_id(initWithNibName:bundle:))]
        fn _init_with_nib_name_bundle(
            this: Allocated<Self>,
            nib_name_or_nil: Option<&NSString>,
            nib_bundle_or_nil: Option<&NSBundle>,
        ) -> Retained<Self> {
            tracing::info!("player controller init");
            let this = this.set_ivars(Ivars::default());
            unsafe {
                msg_send![super(this), initWithNibName: nib_name_or_nil, bundle: nib_bundle_or_nil]
            }
        }

        #[unsafe(method_id(initWithCoder:))]
        fn _init_with_coder(this: Allocated<Self>, coder: &NSCoder) -> Option<Retained<Self>> {
            tracing::info!("player controller init");
            let this = this.set_ivars(Ivars::default());
            unsafe { msg_send![super(this), initWithCoder: coder] }
        }

        #[unsafe(method(loadView))]
        fn _load_view(&self) {
            self.load_view();
        }

        #[unsafe(method(viewDidLoad))]
        fn _view_did_load(&self) {
            // Xcode template calls super at the beginning
            let _: () = unsafe { msg_send![super(self), viewDidLoad] };
            self.view_did_load();
        }

        #[unsafe(method(viewIsAppearing:))]
        fn _view_is_appearing(&self, animated: bool) {
            self.view_is_appearing(animated);
            // Docs say to call super
            let _: () = unsafe { msg_send![super(self), viewIsAppearing: animated] };
        }

        #[unsafe(method(viewWillDisappear:))]
        fn _view_will_disappear(&self, animated: bool) {
            self.view_will_disappear(animated);
            // Docs say to call super
            let _: () = unsafe { msg_send![super(self), viewWillDisappear: animated] };
        }

        #[unsafe(method(viewDidDisappear:))]
        fn _view_did_disappear(&self, animated: bool) {
            self.view_did_disappear(animated);
            // Docs say to call super
            let _: () = unsafe { msg_send![super(self), viewDidDisappear: animated] };
        }

        #[unsafe(method(toggleGamepad:))]
        fn _toggle_gamepad(&self, _sender: Option<&AnyObject>) {
            if let Some(gamepad) = self.ivars().gamepad.get() {
                gamepad.toggle_visibility();
            }
        }
    }

    /// UIResponder
    #[allow(non_snake_case)]
    impl PlayerController {
        #[unsafe(method(canBecomeFirstResponder))]
        fn canBecomeFirstResponder(&self) -> bool {
            true
        }

        #[unsafe(method(becomeFirstResponder))]
        fn becomeFirstResponder(&self) -> bool {
            tracing::info!("player controller becomeFirstResponder");
            self.view().becomeFirstResponder();
            true
        }

        #[unsafe(method(canResignFirstResponder))]
        fn canResignFirstResponder(&self) -> bool {
            true
        }

        #[unsafe(method(resignFirstResponder))]
        fn resignFirstResponder(&self) -> bool {
            tracing::info!("controller resignFirstResponder");
            true
        }
    }
);

impl PlayerController {
    /// For use by run_swf.rs
    pub fn new(
        mtm: MainThreadMarker,
        content: PlayingContent,
        options: PlayerOptions,
    ) -> Retained<Self> {
        let this = mtm.alloc().set_ivars(Ivars {
            content: Cell::new(Some(content)),
            user_options: Cell::new(Some(options)),
            storage_backend: Cell::new(None),
            // run_swf.rs doesn't need security scoping.
            _scoped_resource: Cell::new(None),
            player: OnceCell::new(),
        });
        let nil = None::<&AnyObject>;
        unsafe { msg_send![super(this), initWithNibName: nil, bundle: nil] }
    }

    pub fn empty(mtm: MainThreadMarker) -> Retained<Self> {
        let this = mtm.alloc().set_ivars(Default::default());
        let nil = None::<&AnyObject>;
        unsafe { msg_send![super(this), initWithNibName: nil, bundle: nil] }
    }

    /// Prepare the controller for playing the given movie.
    pub fn setup_movie(&self, movie: &Movie) {
        let nsurl = movie.link();

        self.ivars()
            .content
            .set(Some(storage::get_playing_content(&nsurl)));
        self.ivars().user_options.set(Some(movie.user_options()));
        self.ivars()
            .storage_backend
            .set(Some(Box::new(storage::MovieStorageBackend {
                movie: movie.retain(),
            })));
        self.ivars()._scoped_resource.set(if nsurl.isFileURL() {
            Some(SecurityScopedResource::access(&nsurl).expect("failed accessing NSURL"))
        } else {
            None
        });
    }

    fn load_view(&self) {
        tracing::info!("player loadView");
        let mtm = MainThreadMarker::from(self);
        let view = PlayerView::initWithFrame(
            mtm.alloc(),
            CGRect::new(CGPoint::ZERO, CGSize::new(1.0, 1.0)),
        );
        self.setView(Some(&view));
    }

    fn view_did_load(&self) {
        tracing::info!("player viewDidLoad");

        // TODO: Specify safe area somehow
        let view = self.view();
        let renderer = view.create_renderer();

        let future_spawner = FutureSpawner {
            mtm: self.mtm(),
            main_run_loop: NSRunLoop::mainRunLoop(),
        };

        let content = self.ivars().content.take().unwrap();

        let player_options = self.ivars().user_options.take().unwrap();
        let player_options = match &content {
            PlayingContent::DirectFile(_) => player_options.clone(),
            PlayingContent::Bundle(_, bundle) => player_options.or(&bundle.information().player),
        };

        let movie_url = content.initial_swf_url().clone();
        let navigator = ExternalNavigatorBackend::new(
            player_options
                .base
                .to_owned()
                .unwrap_or_else(|| movie_url.clone()),
            player_options.referer.clone(),
            player_options.cookie.clone(),
            future_spawner,
            None,
            player_options.upgrade_to_https.unwrap_or_default(),
            Default::default(),
            ruffle_core::backend::navigator::SocketMode::Allow,
            Rc::new(content),
            Navigator,
        );

        let mut builder = PlayerBuilder::new()
            .with_renderer(renderer)
            .with_navigator(navigator)
            .with_letterbox(player_options.letterbox.unwrap_or(Letterbox::On))
            .with_max_execution_duration(
                player_options
                    .max_execution_duration
                    .unwrap_or(Duration::MAX),
            )
            .with_quality(player_options.quality.unwrap_or(StageQuality::High))
            .with_align(
                player_options.align.unwrap_or_default(),
                player_options.force_align.unwrap_or_default(),
            )
            .with_scale_mode(
                player_options.scale.unwrap_or_default(),
                player_options.force_scale.unwrap_or_default(),
            )
            .with_load_behavior(
                player_options
                    .load_behavior
                    .unwrap_or(LoadBehavior::Streaming),
            )
            .with_spoofed_url(player_options.spoof_url.clone().map(|url| url.to_string()))
            .with_page_url(player_options.spoof_url.clone().map(|url| url.to_string()))
            .with_player_version(player_options.player_version)
            .with_player_runtime(player_options.player_runtime.unwrap_or_default())
            .with_frame_rate(player_options.frame_rate);

        if player_options.dummy_external_interface.unwrap_or_default() {
            // TODO
        }

        match CpalAudioBackend::new(None) {
            Ok(audio) => builder = builder.with_audio(audio),
            Err(e) => tracing::error!("Unable to create audio device: {e}"),
        }

        if let Some(storage) = self.ivars().storage_backend.take() {
            builder = builder.with_storage(storage);
        }

        let player = builder.build();

        let mut player_lock = player.lock().unwrap();
        player_lock.fetch_root_movie(
            movie_url.to_string(),
            player_options.parameters.to_owned(),
            Box::new(|metadata| {
                eprintln!("got movie: {metadata:?}");
            }),
        );
        drop(player_lock);

        view.set_player(player.clone());
        self.ivars()
            .player
            .set(player)
            .unwrap_or_else(|_| panic!("viewDidLoad once"));

        // Attach responsive Virtual Gamepad overlay
        let mtm = self.mtm();
        let gamepad = VirtualGamepadView::initWithPlayerView(mtm, &view);
        // Autoresize mask: FlexibleWidth (2) | FlexibleHeight (16) = 18
        unsafe {
            let _: () = msg_send![&gamepad, setAutoresizingMask: 18usize];
        }
        view.addSubview(&gamepad);
        self.ivars().gamepad.set(gamepad).ok();

        // Add Toggle Gamepad button in navigation bar
        let nav_item = self.navigationItem();
        let toggle_button = unsafe {
            let title = ns_string!("🎮 Controls");
            let cls = AnyClass::get(c"UIBarButtonItem").unwrap();
            let btn: *mut AnyObject = msg_send![cls, alloc];
            let btn: Retained<AnyObject> = msg_send![
                btn,
                initWithTitle: title,
                style: 0isize,
                target: self,
                action: sel!(toggleGamepad:)
            ];
            btn
        };
        unsafe {
            let _: () = msg_send![&nav_item, setRightBarButtonItem: &toggle_button];
        }
    }

    fn view_is_appearing(&self, _animated: bool) {
        tracing::info!("player viewIsAppearing:");

        self.view().start();
    }

    fn view_will_disappear(&self, _animated: bool) {
        tracing::info!("player viewWillDisappear:");

        self.view().stop();
    }

    fn view_did_disappear(&self, _animated: bool) {
        tracing::info!("player viewDidDisappear:");

        self.view().flush();
    }

    pub fn view(&self) -> Retained<PlayerView> {
        let view = (**self).view().expect("controller loads view");
        view.downcast().expect("must have correct view type")
    }

    #[track_caller]
    pub fn player_lock(&self) -> MutexGuard<'_, Player> {
        self.ivars()
            .player
            .get()
            .expect("player initialized")
            .lock()
            .expect("player lock")
    }
}
