use std::cell::{Cell, OnceCell};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use objc2::rc::{Allocated, Retained};
use objc2::runtime::AnyClass;
use objc2::{define_class, msg_send, sel, ClassType, DefinedClass as _};
use objc2_core_foundation::CGRect;
use objc2_foundation::{
    MainThreadMarker, NSCoder, NSDate, NSObjectProtocol, NSRunLoop, NSRunLoopCommonModes, NSSet,
    NSTimer,
};
use objc2_quartz_core::{CALayer, CALayerDelegate, CAMetalLayer};
use objc2_ui_kit::{
    UIEvent, UIKey, UIPress, UIPressPhase, UIPressesEvent, UITouch, UITouchPhase, UIView,
    UIViewContentMode,
};
use ruffle_core::events::{KeyDescriptor, KeyLocation, LogicalKey, MouseButton, PhysicalKey};
use ruffle_core::{Player, PlayerEvent, ViewportDimensions};
use ruffle_render_wgpu::backend::WgpuRenderBackend;
use ruffle_render_wgpu::target::SwapChainTarget;

#[derive(Default)]
pub struct Ivars {
    player: OnceCell<Arc<Mutex<Player>>>,
    timer: OnceCell<Retained<NSTimer>>,
    last_frame_time: Cell<Option<Instant>>,
}

impl fmt::Debug for Ivars {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ivars")
            .field("timer", &self.timer)
            .field("last_frame_time", &self.last_frame_time)
            .finish_non_exhaustive()
    }
}

define_class!(
    #[unsafe(super(UIView))]
    #[name = "PlayerView"]
    #[ivars = Ivars]
    #[derive(Debug)]
    pub struct PlayerView;

    unsafe impl NSObjectProtocol for PlayerView {}

    /// Initialization.
    impl PlayerView {
        #[unsafe(method_id(initWithFrame:))]
        fn _init_with_frame(this: Allocated<Self>, frame: CGRect) -> Retained<Self> {
            let this = this.set_ivars(Ivars::default());
            let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
            this.init();
            this
        }

        #[unsafe(method_id(initWithCoder:))]
        fn _init_with_coder(this: Allocated<Self>, coder: &NSCoder) -> Retained<Self> {
            let this = this.set_ivars(Ivars::default());
            let this: Retained<Self> = unsafe { msg_send![super(this), initWithCoder: coder] };
            this.init();
            this
        }

        #[unsafe(method(layerClass))]
        fn layer_class() -> &AnyClass {
            CAMetalLayer::class()
        }

        #[unsafe(method(timerFire:))]
        fn _timer_fire(&self, _timer: &NSTimer) {
            self.timer_fire();
        }
    }

    /// UIResponder
    #[allow(non_snake_case)]
    impl PlayerView {
        #[unsafe(method(canBecomeFirstResponder))]
        fn canBecomeFirstResponder(&self) -> bool {
            true
        }

        #[unsafe(method(becomeFirstResponder))]
        fn becomeFirstResponder(&self) -> bool {
            tracing::info!("becomeFirstResponder");
            true
        }

        #[unsafe(method(canResignFirstResponder))]
        fn canResignFirstResponder(&self) -> bool {
            true
        }

        #[unsafe(method(resignFirstResponder))]
        fn resignFirstResponder(&self) -> bool {
            tracing::info!("resignFirstResponder");
            true
        }

        #[unsafe(method(touchesBegan:withEvent:))]
        fn touchesBegan_withEvent(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            tracing::trace!("touchesBegan:withEvent:");
            if !self.handle_touches(touches) {
                // Forward to super
                let _: () =
                    unsafe { msg_send![super(self), touchesBegan: touches, withEvent: event] };
            }
        }

        #[unsafe(method(touchesMoved:withEvent:))]
        fn touchesMoved_withEvent(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            tracing::trace!("touchesMoved:withEvent:");
            if !self.handle_touches(touches) {
                // Forward to super
                let _: () =
                    unsafe { msg_send![super(self), touchesMoved: touches, withEvent: event] };
            }
        }

        #[unsafe(method(touchesEnded:withEvent:))]
        fn touchesEnded_withEvent(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            tracing::trace!("touchesEnded:withEvent:");
            if !self.handle_touches(touches) {
                // Forward to super
                let _: () =
                    unsafe { msg_send![super(self), touchesEnded: touches, withEvent: event] };
            }
        }

        #[unsafe(method(touchesCancelled:withEvent:))]
        fn touchesCancelled_withEvent(&self, touches: &NSSet<UITouch>, event: Option<&UIEvent>) {
            tracing::trace!("touchesCancelled:withEvent:");
            if !self.handle_touches(touches) {
                // Forward to super
                let _: () =
                    unsafe { msg_send![super(self), touchesCancelled: touches, withEvent: event] };
            }
        }

        #[unsafe(method(pressesBegan:withEvent:))]
        fn pressesBegan_withEvent(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            tracing::trace!("pressesBegan:withEvent:");
            if !self.handle_presses(presses) {
                // Forward to super
                let _: () =
                    unsafe { msg_send![super(self), pressesBegan: presses, withEvent: event] };
            }
        }

        #[unsafe(method(pressesChanged:withEvent:))]
        fn pressesChanged_withEvent(
            &self,
            presses: &NSSet<UIPress>,
            event: Option<&UIPressesEvent>,
        ) {
            tracing::trace!("pressesChanged:withEvent:");
            if !self.handle_presses(presses) {
                // Forward to super
                let _: () =
                    unsafe { msg_send![super(self), pressesChanged: presses, withEvent: event] };
            }
        }

        #[unsafe(method(pressesEnded:withEvent:))]
        fn pressesEnded_withEvent(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            tracing::trace!("pressesEnded:withEvent:");
            if !self.handle_presses(presses) {
                // Forward to super
                let _: () =
                    unsafe { msg_send![super(self), pressesEnded: presses, withEvent: event] };
            }
        }

        #[unsafe(method(pressesCancelled:withEvent:))]
        fn pressesCancelled_withEvent(
            &self,
            presses: &NSSet<UIPress>,
            event: Option<&UIPressesEvent>,
        ) {
            tracing::trace!("pressesCancelled:withEvent:");
            if !self.handle_presses(presses) {
                // Forward to super
                let _: () =
                    unsafe { msg_send![super(self), pressesCancelled: presses, withEvent: event] };
            }
        }

        #[unsafe(method(remoteControlReceivedWithEvent:))]
        fn remoteControlReceivedWithEvent(&self, event: Option<&UIEvent>) {
            tracing::info!(subtype = ?event.map(|e| e.subtype()), "remoteControlReceivedWithEvent:");
        }
    }

    /// UIView overrides.
    #[allow(non_snake_case)]
    impl PlayerView {
        #[unsafe(method(canBecomeFocused))]
        fn canBecomeFocused(&self) -> bool {
            tracing::info!("canBecomeFocused");
            true
        }
    }

    // We implement the layer delegate instead of the usual `drawRect:` and
    // `layoutSubviews` methods, since we use a custom `layerClass`, and then
    // UIView won't call those methods.
    //
    // The view is automatically set as the layer's delegate.
    unsafe impl CALayerDelegate for PlayerView {
        #[unsafe(method(displayLayer:))]
        fn _display_layer(&self, _layer: &CALayer) {
            self.draw_rect();
        }

        // This is the recommended way to listen for changes to the layer's
        // frame. Also tracks changes to the backing scale factor.
        //
        // It may be called at other times though, so we check the configured
        // size in `resize` first to avoid unnecessary work.
        #[unsafe(method(layoutSublayersOfLayer:))]
        fn _layout_sublayers_of_layer(&self, _layer: &CALayer) {
            self.resize();
        }
    }
);

impl PlayerView {
    #[allow(non_snake_case)]
    pub fn initWithFrame(this: Allocated<Self>, frame_rect: CGRect) -> Retained<Self> {
        unsafe { msg_send![this, initWithFrame: frame_rect] }
    }

    fn init(&self) {
        // Ensure that the view calls `drawRect:` after being resized
        self.setContentMode(UIViewContentMode::Redraw);

        // Create repeating timer that won't fire until we properly start it
        // (because of the high interval).
        //
        // TODO: Consider running two timers, one to maintain the frame rate,
        // and one to update Flash timers.
        let timer = unsafe {
            NSTimer::timerWithTimeInterval_target_selector_userInfo_repeats(
                f64::MAX,
                self,
                sel!(timerFire:),
                None,
                true,
            )
        };
        // Associate the timer with all run loop modes, so that it runs even
        // when live-resizing or mouse dragging the window.
        unsafe { NSRunLoop::mainRunLoop().addTimer_forMode(&timer, NSRunLoopCommonModes) };
        self.ivars().timer.set(timer).expect("init timer only once");
    }

    pub fn set_player(&self, player: Arc<Mutex<Player>>) {
        // TODO: Use `player.start_time` here to ensure that our deltas are
        // correct.
        self.ivars().last_frame_time.set(Some(Instant::now()));
        self.ivars()
            .player
            .set(player)
            .unwrap_or_else(|_| panic!("only init player once"));
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

    /// Dispatch synthetic key events directly into Ruffle (used by Virtual Gamepad & Controllers)
    pub fn send_key(&self, physical_key: PhysicalKey, logical_key: LogicalKey, is_down: bool) {
        let key = KeyDescriptor {
            physical_key,
            logical_key,
            key_location: KeyLocation::Standard,
        };
        let event = if is_down {
            PlayerEvent::KeyDown { key }
        } else {
            PlayerEvent::KeyUp { key }
        };
        if let Some(player) = self.ivars().player.get() {
            if let Ok(mut player_lock) = player.lock() {
                player_lock.handle_event(event);
            }
        }
    }

    fn resize(&self) {
        tracing::info!("resizing to {:?}", self.frame().size);
        let new_dimensions = self.viewport_dimensions();

        let mut player_lock = self.player_lock();
        // Avoid unnecessary resizes
        // FIXME: Expose `PartialEq` on `ViewportDimensions`.
        let old_dimensions = player_lock.viewport_dimensions();
        if new_dimensions.height != old_dimensions.height
            || new_dimensions.width != old_dimensions.width
            || new_dimensions.scale_factor != old_dimensions.scale_factor
        {
            player_lock.set_viewport_dimensions(new_dimensions);
        }
    }

    fn draw_rect(&self) {
        tracing::trace!("drawing");
        // Render if the system asks for it because of a resize,
        // or if we asked for it with `setNeedsDisplay`.
        self.player_lock().render();
    }

    pub fn viewport_dimensions(&self) -> ViewportDimensions {
        let size = self.frame().size;
        let scale_factor = self.contentScaleFactor();
        ViewportDimensions {
            width: (size.width * scale_factor) as u32,
            height: (size.height * scale_factor) as u32,
            scale_factor: scale_factor as f64,
        }
    }

    pub fn create_renderer(&self) -> WgpuRenderBackend<SwapChainTarget> {
        let layer = self.layer();
        let dimensions = self.viewport_dimensions();
        let layer_ptr = Retained::as_ptr(&layer).cast_mut().cast();
        unsafe {
            WgpuRenderBackend::for_window_unsafe(
                wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(layer_ptr),
                (dimensions.width.max(1), dimensions.height.max(1)),
                wgpu::Backends::METAL,
                wgpu::PowerPreference::HighPerformance,
            )
            .expect("creating renderer")
        }
    }

    #[track_caller]
    pub fn timer(&self) -> &NSTimer {
        self.ivars().timer.get().expect("timer initialized")
    }

    pub fn start(&self) {
        self.player_lock().set_is_playing(true);
        self.timer().fire();
    }

    pub fn stop(&self) {
        self.player_lock().set_is_playing(false);
        // Don't update the timer while we're stopped
        self.timer().setFireDate(&NSDate::distantFuture());
    }

    pub fn flush(&self) {
        self.player_lock().flush_shared_objects();
    }

    fn timer_fire(&self) {
        let last_frame_time = self
            .ivars()
            .last_frame_time
            .get()
            .expect("initialized last frame time");
        let new_time = Instant::now();
        let dt = new_time.duration_since(last_frame_time).as_nanos();
        self.ivars().last_frame_time.set(Some(new_time));
        tracing::trace!("timer fire: {:?}", dt as f64 / 1_000_000.0);

        let mut player_lock = self.player_lock();

        player_lock.tick(dt as f64 / 1_000_000.0);
        // FIXME: The instant that `time_til_next_frame` is relative to isn't
        // defined, so we have to assume that it's roughly relative to "now".
        let next_fire =
            NSDate::dateWithTimeIntervalSinceNow(player_lock.time_til_next_frame().as_secs_f64());
        self.timer().setFireDate(&next_fire);

        if player_lock.needs_render() {
            self.layer().setNeedsDisplay();
        }
    }

    fn handle_touches(&self, touches: &NSSet<UITouch>) -> bool {
        let mut player_lock = self.player_lock();

        // Flash only supports one touch at a time, so we intentially don't set
        // `multipleTouchEnabled`, and don't have to do check all touches here.
        let touch = touches.anyObject().expect("touches must contain a touch");

        let point = touch.locationInView(Some(self));
        let scale_factor = self.contentScaleFactor();
        let x = point.x as f64 * scale_factor;
        let y = point.y as f64 * scale_factor;
        // We don't know which button was pressed in UIKit.
        let button = MouseButton::Left;

        let event_handled = match touch.phase() {
            UITouchPhase::Began => {
                player_lock.set_mouse_in_stage(true);
                player_lock.handle_event(PlayerEvent::MouseDown {
                    x,
                    y,
                    button,
                    // We always know whether a click was a double click or not.
                    index: Some(touch.tapCount()),
                })
            }
            UITouchPhase::Moved => {
                player_lock.set_mouse_in_stage(true);
                player_lock.handle_event(PlayerEvent::MouseMove { x, y })
            }
            UITouchPhase::Ended => {
                player_lock.set_mouse_in_stage(true);
                let up_handled = player_lock.handle_event(PlayerEvent::MouseUp { x, y, button });
                player_lock.set_mouse_in_stage(false);
                up_handled || player_lock.handle_event(PlayerEvent::MouseLeave)
            }
            UITouchPhase::Cancelled => {
                player_lock.set_mouse_in_stage(true);
                player_lock.handle_event(PlayerEvent::MouseLeave)
            }
            _ => return false,
        };

        if player_lock.needs_render() {
            self.layer().setNeedsDisplay();
        }

        event_handled
    }

    fn handle_presses(&self, presses: &NSSet<UIPress>) -> bool {
        let mtm = MainThreadMarker::from(self);
        let mut player_lock = self.player_lock();

        let mut handled = false;
        for press in presses {
            // TODO: Consider press.r#type()
            let Some(key) = press.key(mtm) else {
                continue;
            };
            let key = KeyDescriptor {
                physical_key: key_to_physical(&key),
                logical_key: key_to_logical(&key),
                key_location: KeyLocation::Standard,
            };

            let event = match press.phase() {
                UIPressPhase::Began => PlayerEvent::KeyDown { key },
                // FIXME: Forward event cancellation
                UIPressPhase::Ended | UIPressPhase::Cancelled => PlayerEvent::KeyUp { key },
                _ => continue,
            };

            handled |= player_lock.handle_event(event);
        }
        handled
    }
}

impl Drop for PlayerView {
    fn drop(&mut self) {
        // Invalidate the timer if it was registered
        if let Some(timer) = self.ivars().timer.get() {
            timer.invalidate();
        }
    }
}

fn key_to_physical(key: &UIKey) -> PhysicalKey {
    use objc2_ui_kit::UIKeyboardHIDUsage as UI;
    match key.keyCode() {
        UI::KeyboardA => PhysicalKey::KeyA,
        UI::KeyboardB => PhysicalKey::KeyB,
        UI::KeyboardC => PhysicalKey::KeyC,
        UI::KeyboardD => PhysicalKey::KeyD,
        UI::KeyboardE => PhysicalKey::KeyE,
        UI::KeyboardF => PhysicalKey::KeyF,
        UI::KeyboardG => PhysicalKey::KeyG,
        UI::KeyboardH => PhysicalKey::KeyH,
        UI::KeyboardI => PhysicalKey::KeyI,
        UI::KeyboardJ => PhysicalKey::KeyJ,
        UI::KeyboardK => PhysicalKey::KeyK,
        UI::KeyboardL => PhysicalKey::KeyL,
        UI::KeyboardM => PhysicalKey::KeyM,
        UI::KeyboardN => PhysicalKey::KeyN,
        UI::KeyboardO => PhysicalKey::KeyO,
        UI::KeyboardP => PhysicalKey::KeyP,
        UI::KeyboardQ => PhysicalKey::KeyQ,
        UI::KeyboardR => PhysicalKey::KeyR,
        UI::KeyboardS => PhysicalKey::KeyS,
        UI::KeyboardT => PhysicalKey::KeyT,
        UI::KeyboardU => PhysicalKey::KeyU,
        UI::KeyboardV => PhysicalKey::KeyV,
        UI::KeyboardW => PhysicalKey::KeyW,
        UI::KeyboardX => PhysicalKey::KeyX,
        UI::KeyboardY => PhysicalKey::KeyY,
        UI::KeyboardZ => PhysicalKey::KeyZ,
        UI::Keyboard1 => PhysicalKey::Digit1,
        UI::Keyboard2 => PhysicalKey::Digit2,
        UI::Keyboard3 => PhysicalKey::Digit3,
        UI::Keyboard4 => PhysicalKey::Digit4,
        UI::Keyboard5 => PhysicalKey::Digit5,
        UI::Keyboard6 => PhysicalKey::Digit6,
        UI::Keyboard7 => PhysicalKey::Digit7,
        UI::Keyboard8 => PhysicalKey::Digit8,
        UI::Keyboard9 => PhysicalKey::Digit9,
        UI::Keyboard0 => PhysicalKey::Digit0,
        UI::KeyboardReturnOrEnter => PhysicalKey::Enter,
        UI::KeyboardEscape => PhysicalKey::Escape,
        UI::KeyboardDeleteOrBackspace => PhysicalKey::Delete,
        UI::KeyboardTab => PhysicalKey::Tab,
        UI::KeyboardSpacebar => PhysicalKey::Space,
        UI::KeyboardHyphen => PhysicalKey::Minus,
        UI::KeyboardEqualSign => PhysicalKey::Equal,
        UI::KeyboardOpenBracket => PhysicalKey::BracketLeft,
        UI::KeyboardCloseBracket => PhysicalKey::BracketRight,
        UI::KeyboardBackslash => PhysicalKey::Backslash,
        UI::KeyboardSemicolon => PhysicalKey::Semicolon,
        UI::KeyboardQuote => PhysicalKey::Quote,
        UI::KeyboardGraveAccentAndTilde => PhysicalKey::Backquote,
        UI::KeyboardComma => PhysicalKey::Comma,
        UI::KeyboardPeriod => PhysicalKey::Period,
        UI::KeyboardSlash => PhysicalKey::Slash,
        UI::KeyboardCapsLock => PhysicalKey::CapsLock,
        UI::KeyboardF1 => PhysicalKey::F1,
        UI::KeyboardF2 => PhysicalKey::F2,
        UI::KeyboardF3 => PhysicalKey::F3,
        UI::KeyboardF4 => PhysicalKey::F4,
        UI::KeyboardF5 => PhysicalKey::F5,
        UI::KeyboardF6 => PhysicalKey::F6,
        UI::KeyboardF7 => PhysicalKey::F7,
        UI::KeyboardF8 => PhysicalKey::F8,
        UI::KeyboardF9 => PhysicalKey::F9,
        UI::KeyboardF10 => PhysicalKey::F10,
        UI::KeyboardF11 => PhysicalKey::F11,
        UI::KeyboardF12 => PhysicalKey::F12,
        UI::KeyboardScrollLock => PhysicalKey::ScrollLock,
        UI::KeyboardPause => PhysicalKey::Pause,
        UI::KeyboardInsert => PhysicalKey::Insert,
        UI::KeyboardHome => PhysicalKey::Home,
        UI::KeyboardPageUp => PhysicalKey::PageUp,
        UI::KeyboardEnd => PhysicalKey::End,
        UI::KeyboardPageDown => PhysicalKey::PageDown,
        UI::KeyboardRightArrow => PhysicalKey::ArrowRight,
        UI::KeyboardLeftArrow => PhysicalKey::ArrowLeft,
        UI::KeyboardDownArrow => PhysicalKey::ArrowDown,
        UI::KeyboardUpArrow => PhysicalKey::ArrowUp,
        UI::KeypadNumLock => PhysicalKey::NumLock,
        UI::KeypadSlash => PhysicalKey::NumpadDivide,
        UI::KeypadAsterisk => PhysicalKey::NumpadMultiply,
        UI::KeypadHyphen => PhysicalKey::NumpadSubtract,
        UI::KeypadPlus => PhysicalKey::NumpadAdd,
        UI::KeypadEnter => PhysicalKey::NumpadEnter,
        UI::Keypad1 => PhysicalKey::Numpad1,
        UI::Keypad2 => PhysicalKey::Numpad2,
        UI::Keypad3 => PhysicalKey::Numpad3,
        UI::Keypad4 => PhysicalKey::Numpad4,
        UI::Keypad5 => PhysicalKey::Numpad5,
        UI::Keypad6 => PhysicalKey::Numpad6,
        UI::Keypad7 => PhysicalKey::Numpad7,
        UI::Keypad8 => PhysicalKey::Numpad8,
        UI::Keypad9 => PhysicalKey::Numpad9,
        UI::Keypad0 => PhysicalKey::Numpad0,
        UI::KeypadPeriod => PhysicalKey::NumpadComma, // Maybe?
        UI::KeyboardNonUSBackslash => PhysicalKey::IntlBackslash,
        UI::KeypadEqualSign => PhysicalKey::Equal,
        UI::KeyboardF13 => PhysicalKey::F13,
        UI::KeyboardF14 => PhysicalKey::F14,
        UI::KeyboardF15 => PhysicalKey::F15,
        UI::KeyboardF16 => PhysicalKey::F16,
        UI::KeyboardF17 => PhysicalKey::F17,
        UI::KeyboardF18 => PhysicalKey::F18,
        UI::KeyboardF19 => PhysicalKey::F19,
        UI::KeyboardF20 => PhysicalKey::F20,
        UI::KeyboardF21 => PhysicalKey::F21,
        UI::KeyboardF22 => PhysicalKey::F22,
        UI::KeyboardF23 => PhysicalKey::F23,
        UI::KeyboardF24 => PhysicalKey::F24,
        UI::KeypadComma => PhysicalKey::Comma,
        UI::KeypadEqualSignAS400 => PhysicalKey::Equal,
        UI::KeyboardReturn => PhysicalKey::Enter,
        UI::KeyboardLeftControl => PhysicalKey::ControlLeft,
        UI::KeyboardLeftShift => PhysicalKey::ShiftLeft,
        UI::KeyboardLeftAlt => PhysicalKey::AltLeft,
        UI::KeyboardLeftGUI => PhysicalKey::SuperLeft,
        UI::KeyboardRightControl => PhysicalKey::ControlRight,
        UI::KeyboardRightShift => PhysicalKey::ShiftRight,
        UI::KeyboardRightAlt => PhysicalKey::AltRight,
        UI::KeyboardRightGUI => PhysicalKey::SuperRight,
        code => {
            tracing::warn!("unhandled physical key {code:?}");
            PhysicalKey::Unknown
        }
    }
}

fn key_to_logical(key: &UIKey) -> LogicalKey {
    // FIXME: `last()` is functionally equivalent in most cases, but
    // we may want to do something else here.
    let key_char = key.charactersIgnoringModifiers().to_string().chars().last();

    if let Some(key_char) = key_char {
        LogicalKey::Character(key_char)
    } else {
        tracing::warn!("unhandled logical key {key:?}");
        LogicalKey::Unknown
    }
}
