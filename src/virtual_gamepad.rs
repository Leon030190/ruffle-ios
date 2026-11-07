//! Virtual Touch Gamepad for Ruffle iOS
//!
//! Provides ultra-responsive, low-latency on-screen touch controls:
//! - 8-way responsive D-Pad (Up, Down, Left, Right + Diagonals)
//! - Action Buttons: A (Space/Jump), B (Z/Action), X (X/Special), Start (Enter)
//! - Multi-touch tracking: each finger is independently tracked
//! - Intelligent Hit-Testing: Touches outside controls pass through to the game canvas
//! - Tactile Haptic Feedback via iOS Taptic Engine
//! - Customizable sleek translucent Glassmorphism UI

use std::cell::{Cell, OnceCell};
use std::f64::consts::PI;

use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, Bool};
use objc2::{define_class, msg_send, sel, ClassType, DefinedClass as _, Message};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{MainThreadMarker, NSCoder, NSObjectProtocol, NSSet, NSString};
use objc2_ui_kit::{
    UIBezierPath, UIColor, UIEvent, UITouch, UITouchPhase, UIView, UIViewContentMode,
};
use ruffle_core::events::{KeyDescriptor, KeyLocation, LogicalKey, PhysicalKey};

use crate::PlayerView;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct DpadState {
    pub up: bool,
    pub down: bool,
    pub left: bool,
    pub right: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct DpadGeometry {
    pub center: CGPoint,
    pub radius: f64,
    pub deadzone: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct ButtonGeometry {
    pub center: CGPoint,
    pub radius: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct GamepadLayout {
    pub dpad: DpadGeometry,
    pub btn_a: ButtonGeometry,
    pub btn_b: ButtonGeometry,
    pub btn_x: ButtonGeometry,
    pub btn_start: ButtonGeometry,
}

pub struct Ivars {
    player_view: OnceCell<Retained<PlayerView>>,

    // Touch tracking (by pointer address for zero-collision multi-touch)
    active_dpad_touch: Cell<Option<usize>>,
    active_btn_a_touch: Cell<Option<usize>>,
    active_btn_b_touch: Cell<Option<usize>>,
    active_btn_x_touch: Cell<Option<usize>>,
    active_btn_start_touch: Cell<Option<usize>>,

    // Button states
    dpad_state: Cell<DpadState>,
    btn_a_pressed: Cell<bool>,
    btn_b_pressed: Cell<bool>,
    btn_x_pressed: Cell<bool>,
    btn_start_pressed: Cell<bool>,
}

impl std::fmt::Debug for Ivars {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ivars").finish_non_exhaustive()
    }
}

impl Default for Ivars {
    fn default() -> Self {
        Self {
            player_view: OnceCell::new(),
            active_dpad_touch: Cell::new(None),
            active_btn_a_touch: Cell::new(None),
            active_btn_b_touch: Cell::new(None),
            active_btn_x_touch: Cell::new(None),
            active_btn_start_touch: Cell::new(None),
            dpad_state: Cell::new(DpadState::default()),
            btn_a_pressed: Cell::new(false),
            btn_b_pressed: Cell::new(false),
            btn_x_pressed: Cell::new(false),
            btn_start_pressed: Cell::new(false),
        }
    }
}

define_class!(
    #[unsafe(super(UIView))]
    #[name = "VirtualGamepadView"]
    #[ivars = Ivars]
    #[derive(Debug)]
    pub struct VirtualGamepadView;

    unsafe impl NSObjectProtocol for VirtualGamepadView {}

    impl VirtualGamepadView {
        #[unsafe(method_id(initWithFrame:))]
        fn _init_with_frame(this: Allocated<Self>, frame: CGRect) -> Retained<Self> {
            let this = this.set_ivars(Ivars::default());
            let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
            this.init();
            this
        }

        #[unsafe(method_id(initWithCoder:))]
        fn _init_with_coder(this: Allocated<Self>, coder: &NSCoder) -> Option<Retained<Self>> {
            let this = this.set_ivars(Ivars::default());
            let this: Option<Retained<Self>> =
                unsafe { msg_send![super(this), initWithCoder: coder] };
            if let Some(this) = &this {
                this.init();
            }
            this
        }

        #[unsafe(method(drawRect:))]
        fn _draw_rect(&self, rect: CGRect) {
            self.draw_gamepad(rect);
        }

        #[unsafe(method(pointInside:withEvent:))]
        fn _point_inside_with_event(&self, point: CGPoint, _event: Option<&UIEvent>) -> Bool {
            if self.isHidden() {
                return Bool::NO;
            }
            Bool::new(self.is_point_in_controls(point))
        }

        #[unsafe(method(touchesBegan:withEvent:))]
        fn _touches_began(&self, touches: &NSSet<UITouch>, _event: Option<&UIEvent>) {
            self.handle_touches(touches, UITouchPhase::Began);
        }

        #[unsafe(method(touchesMoved:withEvent:))]
        fn _touches_moved(&self, touches: &NSSet<UITouch>, _event: Option<&UIEvent>) {
            self.handle_touches(touches, UITouchPhase::Moved);
        }

        #[unsafe(method(touchesEnded:withEvent:))]
        fn _touches_ended(&self, touches: &NSSet<UITouch>, _event: Option<&UIEvent>) {
            self.handle_touches(touches, UITouchPhase::Ended);
        }

        #[unsafe(method(touchesCancelled:withEvent:))]
        fn _touches_cancelled(&self, touches: &NSSet<UITouch>, _event: Option<&UIEvent>) {
            self.handle_touches(touches, UITouchPhase::Cancelled);
        }
    }
);

impl VirtualGamepadView {
    pub fn new(mtm: MainThreadMarker, frame: CGRect) -> Retained<Self> {
        let this = mtm.alloc();
        unsafe { msg_send![this, initWithFrame: frame] }
    }

    pub fn initWithPlayerView(
        mtm: MainThreadMarker,
        player_view: &PlayerView,
    ) -> Retained<Self> {
        let frame = player_view.bounds();
        let this = Self::new(mtm, frame);
        this.set_player_view(player_view);
        this
    }

    fn init(&self) {
        // Multi-touch must be enabled for simultaneous D-pad and button control
        self.setMultipleTouchEnabled(true);
        self.setUserInteractionEnabled(true);
        self.setContentMode(UIViewContentMode::Redraw);

        // Transparent background so the underlying Flash game remains visible
        let clear_color = UIColor::clearColor();
        self.setBackgroundColor(Some(&clear_color));
        self.setOpaque(false);
    }

    pub fn set_player_view(&self, player_view: &PlayerView) {
        self.ivars().player_view.set(player_view.retain()).ok();
    }

    fn get_player_view(&self) -> Option<Retained<PlayerView>> {
        if let Some(pv) = self.ivars().player_view.get() {
            return Some(pv.clone());
        }
        // Fallback: check superview
        if let Some(superview) = self.superview() {
            if let Ok(pv) = superview.downcast::<PlayerView>() {
                return Some(pv);
            }
        }
        None
    }

    pub fn toggle_visibility(&self) {
        let hidden = self.isHidden();
        self.setHidden(!hidden);
        trigger_haptic(1); // Light haptic feedback
    }

    /// Calculate responsive control locations based on current view bounds
    pub fn layout_geometry(&self, bounds: CGRect) -> GamepadLayout {
        let w = bounds.size.width;
        let h = bounds.size.height;
        let pad_bottom = 28.0;

        // D-Pad on bottom-left
        let dpad = DpadGeometry {
            center: CGPoint::new(95.0, h - 95.0 - pad_bottom),
            radius: 70.0,
            deadzone: 16.0,
        };

        // Action Buttons on bottom-right (SNES diamond layout)
        // Button A (Jump / Space - bottom)
        let btn_a = ButtonGeometry {
            center: CGPoint::new(w - 70.0, h - 70.0 - pad_bottom),
            radius: 32.0,
        };

        // Button B (Action / Z - left)
        let btn_b = ButtonGeometry {
            center: CGPoint::new(w - 145.0, h - 90.0 - pad_bottom),
            radius: 30.0,
        };

        // Button X (Special / X - top)
        let btn_x = ButtonGeometry {
            center: CGPoint::new(w - 70.0, h - 145.0 - pad_bottom),
            radius: 28.0,
        };

        // Button Start (Enter / Confirm - top-left)
        let btn_start = ButtonGeometry {
            center: CGPoint::new(w - 145.0, h - 155.0 - pad_bottom),
            radius: 22.0,
        };

        GamepadLayout {
            dpad,
            btn_a,
            btn_b,
            btn_x,
            btn_start,
        }
    }

    /// Smart Hit-Testing: Only touches hitting controls are consumed.
    /// Touches outside pass through to the game canvas for point-and-click.
    fn is_point_in_controls(&self, p: CGPoint) -> bool {
        let geom = self.layout_geometry(self.bounds());

        // D-Pad zone (with generous touch margin)
        let dpad_dist_sq = (p.x - geom.dpad.center.x).powi(2) + (p.y - geom.dpad.center.y).powi(2);
        if dpad_dist_sq <= (geom.dpad.radius + 30.0).powi(2) {
            return true;
        }

        // Button A
        let da = (p.x - geom.btn_a.center.x).powi(2) + (p.y - geom.btn_a.center.y).powi(2);
        if da <= (geom.btn_a.radius + 20.0).powi(2) {
            return true;
        }

        // Button B
        let db = (p.x - geom.btn_b.center.x).powi(2) + (p.y - geom.btn_b.center.y).powi(2);
        if db <= (geom.btn_b.radius + 20.0).powi(2) {
            return true;
        }

        // Button X
        let dx = (p.x - geom.btn_x.center.x).powi(2) + (p.y - geom.btn_x.center.y).powi(2);
        if dx <= (geom.btn_x.radius + 20.0).powi(2) {
            return true;
        }

        // Button Start
        let ds = (p.x - geom.btn_start.center.x).powi(2) + (p.y - geom.btn_start.center.y).powi(2);
        if ds <= (geom.btn_start.radius + 20.0).powi(2) {
            return true;
        }

        false
    }

    /// Fast, low-latency multi-touch dispatcher
    fn handle_touches(&self, touches: &NSSet<UITouch>, phase: UITouchPhase) {
        let geom = self.layout_geometry(self.bounds());
        let ivars = self.ivars();
        let mut needs_redraw = false;

        for touch in touches {
            let touch_ptr = Retained::as_ptr(&touch) as usize;
            let point = touch.locationInView(Some(self));

            match phase {
                UITouchPhase::Began => {
                    // Check if touch is on D-Pad
                    let dpad_dist_sq = (point.x - geom.dpad.center.x).powi(2)
                        + (point.y - geom.dpad.center.y).powi(2);
                    if dpad_dist_sq <= (geom.dpad.radius + 30.0).powi(2)
                        && ivars.active_dpad_touch.get().is_none()
                    {
                        ivars.active_dpad_touch.set(Some(touch_ptr));
                        self.update_dpad_from_point(point, geom.dpad);
                        needs_redraw = true;
                        continue;
                    }

                    // Check Button A
                    let da = (point.x - geom.btn_a.center.x).powi(2)
                        + (point.y - geom.btn_a.center.y).powi(2);
                    if da <= (geom.btn_a.radius + 20.0).powi(2)
                        && ivars.active_btn_a_touch.get().is_none()
                    {
                        ivars.active_btn_a_touch.set(Some(touch_ptr));
                        ivars.btn_a_pressed.set(true);
                        self.send_button_event(PhysicalKey::Space, LogicalKey::Character(' '), true);
                        trigger_haptic(2); // Medium impact
                        needs_redraw = true;
                        continue;
                    }

                    // Check Button B
                    let db = (point.x - geom.btn_b.center.x).powi(2)
                        + (point.y - geom.btn_b.center.y).powi(2);
                    if db <= (geom.btn_b.radius + 20.0).powi(2)
                        && ivars.active_btn_b_touch.get().is_none()
                    {
                        ivars.active_btn_b_touch.set(Some(touch_ptr));
                        ivars.btn_b_pressed.set(true);
                        self.send_button_event(PhysicalKey::KeyZ, LogicalKey::Character('z'), true);
                        trigger_haptic(2);
                        needs_redraw = true;
                        continue;
                    }

                    // Check Button X
                    let dx = (point.x - geom.btn_x.center.x).powi(2)
                        + (point.y - geom.btn_x.center.y).powi(2);
                    if dx <= (geom.btn_x.radius + 20.0).powi(2)
                        && ivars.active_btn_x_touch.get().is_none()
                    {
                        ivars.active_btn_x_touch.set(Some(touch_ptr));
                        ivars.btn_x_pressed.set(true);
                        self.send_button_event(PhysicalKey::KeyX, LogicalKey::Character('x'), true);
                        trigger_haptic(2);
                        needs_redraw = true;
                        continue;
                    }

                    // Check Button Start
                    let ds = (point.x - geom.btn_start.center.x).powi(2)
                        + (point.y - geom.btn_start.center.y).powi(2);
                    if ds <= (geom.btn_start.radius + 20.0).powi(2)
                        && ivars.active_btn_start_touch.get().is_none()
                    {
                        ivars.active_btn_start_touch.set(Some(touch_ptr));
                        ivars.btn_start_pressed.set(true);
                        self.send_button_event(PhysicalKey::Enter, LogicalKey::Character('\r'), true);
                        trigger_haptic(1);
                        needs_redraw = true;
                        continue;
                    }
                }
                UITouchPhase::Moved => {
                    // Update D-Pad if this touch belongs to D-Pad
                    if ivars.active_dpad_touch.get() == Some(touch_ptr) {
                        self.update_dpad_from_point(point, geom.dpad);
                        needs_redraw = true;
                    }
                }
                UITouchPhase::Ended | UITouchPhase::Cancelled => {
                    // Release D-Pad
                    if ivars.active_dpad_touch.get() == Some(touch_ptr) {
                        ivars.active_dpad_touch.set(None);
                        self.set_dpad_state(DpadState::default());
                        needs_redraw = true;
                    }
                    // Release Button A
                    if ivars.active_btn_a_touch.get() == Some(touch_ptr) {
                        ivars.active_btn_a_touch.set(None);
                        if ivars.btn_a_pressed.get() {
                            ivars.btn_a_pressed.set(false);
                            self.send_button_event(PhysicalKey::Space, LogicalKey::Character(' '), false);
                            needs_redraw = true;
                        }
                    }
                    // Release Button B
                    if ivars.active_btn_b_touch.get() == Some(touch_ptr) {
                        ivars.active_btn_b_touch.set(None);
                        if ivars.btn_b_pressed.get() {
                            ivars.btn_b_pressed.set(false);
                            self.send_button_event(PhysicalKey::KeyZ, LogicalKey::Character('z'), false);
                            needs_redraw = true;
                        }
                    }
                    // Release Button X
                    if ivars.active_btn_x_touch.get() == Some(touch_ptr) {
                        ivars.active_btn_x_touch.set(None);
                        if ivars.btn_x_pressed.get() {
                            ivars.btn_x_pressed.set(false);
                            self.send_button_event(PhysicalKey::KeyX, LogicalKey::Character('x'), false);
                            needs_redraw = true;
                        }
                    }
                    // Release Button Start
                    if ivars.active_btn_start_touch.get() == Some(touch_ptr) {
                        ivars.active_btn_start_touch.set(None);
                        if ivars.btn_start_pressed.get() {
                            ivars.btn_start_pressed.set(false);
                            self.send_button_event(PhysicalKey::Enter, LogicalKey::Character('\r'), false);
                            needs_redraw = true;
                        }
                    }
                }
                _ => {}
            }
        }

        if needs_redraw {
            self.setNeedsDisplay();
        }
    }

    /// 8-way directional deadzone math
    fn update_dpad_from_point(&self, point: CGPoint, dpad: DpadGeometry) {
        let dx = point.x - dpad.center.x;
        let dy = point.y - dpad.center.y;
        let dist = (dx * dx + dy * dy).sqrt();

        if dist < dpad.deadzone {
            self.set_dpad_state(DpadState::default());
            return;
        }

        let angle = dy.atan2(dx); // [-PI, PI]
        let deg = angle * (180.0 / PI);

        // 8-way directional sectors
        let state = match deg {
            // Right: -22.5 to 22.5
            -22.5..=22.5 => DpadState {
                up: false,
                down: false,
                left: false,
                right: true,
            },
            // Down-Right: 22.5 to 67.5
            22.5..=67.5 => DpadState {
                up: false,
                down: true,
                left: false,
                right: true,
            },
            // Down: 67.5 to 112.5
            67.5..=112.5 => DpadState {
                up: false,
                down: true,
                left: false,
                right: false,
            },
            // Down-Left: 112.5 to 157.5
            112.5..=157.5 => DpadState {
                up: false,
                down: true,
                left: true,
                right: false,
            },
            // Up-Right: -67.5 to -22.5
            -67.5..=-22.5 => DpadState {
                up: true,
                down: false,
                left: false,
                right: true,
            },
            // Up: -112.5 to -67.5
            -112.5..=-67.5 => DpadState {
                up: true,
                down: false,
                left: false,
                right: false,
            },
            // Up-Left: -157.5 to -112.5
            -157.5..=-112.5 => DpadState {
                up: true,
                down: false,
                left: true,
                right: false,
            },
            // Left: > 157.5 or < -157.5
            _ => DpadState {
                up: false,
                down: false,
                left: true,
                right: false,
            },
        };

        self.set_dpad_state(state);
    }

    fn set_dpad_state(&self, new_state: DpadState) {
        let old_state = self.ivars().dpad_state.get();
        if old_state == new_state {
            return;
        }

        self.ivars().dpad_state.set(new_state);

        let mut changed = false;
        // Up
        if old_state.up != new_state.up {
            self.send_button_event(PhysicalKey::ArrowUp, LogicalKey::Character(' '), new_state.up);
            changed = true;
        }
        // Down
        if old_state.down != new_state.down {
            self.send_button_event(PhysicalKey::ArrowDown, LogicalKey::Character(' '), new_state.down);
            changed = true;
        }
        // Left
        if old_state.left != new_state.left {
            self.send_button_event(PhysicalKey::ArrowLeft, LogicalKey::Character(' '), new_state.left);
            changed = true;
        }
        // Right
        if old_state.right != new_state.right {
            self.send_button_event(PhysicalKey::ArrowRight, LogicalKey::Character(' '), new_state.right);
            changed = true;
        }

        if changed && (new_state.up || new_state.down || new_state.left || new_state.right) {
            trigger_haptic(1);
        }
    }

    fn send_button_event(
        &self,
        physical_key: PhysicalKey,
        logical_key: LogicalKey,
        is_down: bool,
    ) {
        if let Some(player_view) = self.get_player_view() {
            player_view.send_key(physical_key, logical_key, is_down);
        }
    }

    /// Sleek glassmorphic on-screen rendering
    fn draw_gamepad(&self, _rect: CGRect) {
        let geom = self.layout_geometry(self.bounds());
        let dpad = self.ivars().dpad_state.get();

        // Colors
        let dpad_bg = UIColor::colorWithWhite_alpha(0.05, 0.35);
        let dpad_border = UIColor::colorWithWhite_alpha(1.0, 0.25);
        let normal_dir = UIColor::colorWithWhite_alpha(1.0, 0.45);
        let active_dir = UIColor::colorWithRed_green_blue_alpha(1.0, 0.58, 0.16, 0.95); // Ruffle Orange

        // 1. Draw D-Pad Outer Ring
        dpad_bg.setFill();
        dpad_border.setStroke();
        let dpad_ring = UIBezierPath::bezierPathWithOvalInRect(CGRect::new(
            CGPoint::new(geom.dpad.center.x - geom.dpad.radius, geom.dpad.center.y - geom.dpad.radius),
            CGSize::new(geom.dpad.radius * 2.0, geom.dpad.radius * 2.0),
        ));
        dpad_ring.setLineWidth(2.0);
        dpad_ring.fill();
        dpad_ring.stroke();

        // 2. Draw D-pad Direction Arrows (Up, Down, Left, Right)
        let draw_arrow = |cx: f64, cy: f64, is_active: bool, is_horizontal: bool| {
            if is_active {
                active_dir.setFill();
            } else {
                normal_dir.setFill();
            }
            let (w, h) = if is_horizontal { (22.0, 14.0) } else { (14.0, 22.0) };
            let arrow_rect = CGRect::new(CGPoint::new(cx - w / 2.0, cy - h / 2.0), CGSize::new(w, h));
            let arrow = UIBezierPath::bezierPathWithRoundedRect_cornerRadius(arrow_rect, 4.0);
            arrow.fill();
        };

        let offset = 42.0;
        draw_arrow(geom.dpad.center.x, geom.dpad.center.y - offset, dpad.up, false);
        draw_arrow(geom.dpad.center.x, geom.dpad.center.y + offset, dpad.down, false);
        draw_arrow(geom.dpad.center.x - offset, geom.dpad.center.y, dpad.left, true);
        draw_arrow(geom.dpad.center.x + offset, geom.dpad.center.y, dpad.right, true);

        // 3. Draw Action Buttons (A, B, X, Start)
        let draw_button = |center: CGPoint, radius: f64, is_pressed: bool, active_color: &UIColor| {
            if is_pressed {
                active_color.setFill();
            } else {
                UIColor::colorWithWhite_alpha(0.12, 0.45).setFill();
            }
            UIColor::colorWithWhite_alpha(1.0, 0.35).setStroke();

            let btn_rect = CGRect::new(
                CGPoint::new(center.x - radius, center.y - radius),
                CGSize::new(radius * 2.0, radius * 2.0),
            );
            let btn_path = UIBezierPath::bezierPathWithOvalInRect(btn_rect);
            btn_path.setLineWidth(2.0);
            btn_path.fill();
            btn_path.stroke();
        };

        let btn_a_color = UIColor::colorWithRed_green_blue_alpha(0.18, 0.55, 0.98, 0.9); // Ruffle Blue
        let btn_b_color = UIColor::colorWithRed_green_blue_alpha(0.95, 0.35, 0.25, 0.9); // Crimson
        let btn_x_color = UIColor::colorWithRed_green_blue_alpha(0.25, 0.85, 0.45, 0.9); // Green
        let btn_s_color = UIColor::colorWithRed_green_blue_alpha(0.85, 0.65, 0.20, 0.9); // Gold

        draw_button(geom.btn_a.center, geom.btn_a.radius, self.ivars().btn_a_pressed.get(), &btn_a_color);
        draw_button(geom.btn_b.center, geom.btn_b.radius, self.ivars().btn_b_pressed.get(), &btn_b_color);
        draw_button(geom.btn_x.center, geom.btn_x.radius, self.ivars().btn_x_pressed.get(), &btn_x_color);
        draw_button(geom.btn_start.center, geom.btn_start.radius, self.ivars().btn_start_pressed.get(), &btn_s_color);
    }
}

/// Trigger native iOS Taptic Engine feedback
fn trigger_haptic(style: isize) {
    if let Some(cls) = AnyClass::get(c"UIImpactFeedbackGenerator") {
        unsafe {
            let gen: *mut AnyObject = msg_send![cls, alloc];
            let gen: *mut AnyObject = msg_send![gen, initWithStyle: style];
            let _: () = msg_send![gen, prepare];
            let _: () = msg_send![gen, impactOccurred];
            let _: () = msg_send![gen, release];
        }
    }
}
