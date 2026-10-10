//! Wayland idle inhibition (`zwp_idle_inhibit_v1`).
//!
//! Compositors that manage idle themselves (via `ext-idle-notify-v1`) ignore
//! the D-Bus inhibitors `keepawake` uses on Linux, but do honor the Wayland
//! idle-inhibit protocol. While a video plays this holds an inhibitor on the
//! app's own surface.

#[cfg(target_os = "linux")]
mod imp {
    use std::sync::OnceLock;

    use slint::winit_030::winit::{
        raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle},
        window::Window,
    };
    use wayland_backend::client::Backend;
    use wayland_client::{
        Connection, Dispatch, EventQueue, Proxy, QueueHandle,
        backend::ObjectId,
        protocol::{wl_registry, wl_surface},
    };
    use wayland_protocols::wp::idle_inhibit::zv1::client::{
        zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1,
        zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1,
    };

    /// `(wl_display, wl_surface)` from the winit window, stored as integers to
    /// keep the global `Send`/`Sync`. Captured once on the UI thread.
    static HANDLES: OnceLock<(usize, usize)> = OnceLock::new();

    /// Guest view of winit's Wayland connection; `None` when the session is not
    /// Wayland or the compositor lacks idle-inhibit support.
    static WAYLAND: OnceLock<Option<Wayland>> = OnceLock::new();

    struct Wayland {
        connection: Connection,
        queue: EventQueue<Init>,
        manager: ZwpIdleInhibitManagerV1,
        surface: wl_surface::WlSurface,
    }

    /// A live idle inhibitor. Dropping it releases the hold.
    pub struct Inhibitor {
        inhibitor: ZwpIdleInhibitorV1,
        connection: Connection,
    }

    impl Drop for Inhibitor {
        fn drop(&mut self) {
            self.inhibitor.destroy();
            let _ = self.connection.flush();
        }
    }

    /// Capture the window's Wayland handles. Safe to call repeatedly; only the
    /// first successful capture takes effect.
    pub fn capture(window: &Window) {
        let Some(handles) = raw_handles(window) else {
            return;
        };
        if HANDLES.set(handles).is_ok() {
            let _ = WAYLAND.set(build_wayland(handles.0, handles.1));
        }
    }

    /// Arm an idle inhibitor on the captured surface, or `None` when the
    /// compositor has no idle-inhibit support (or no Wayland window exists yet).
    pub fn arm() -> Option<Inhibitor> {
        let wayland = WAYLAND.get()?.as_ref()?;
        let inhibitor =
            wayland
                .manager
                .create_inhibitor(&wayland.surface, &wayland.queue.handle(), ());
        wayland.connection.flush().ok()?;
        Some(Inhibitor {
            inhibitor,
            connection: wayland.connection.clone(),
        })
    }

    fn raw_handles(window: &Window) -> Option<(usize, usize)> {
        let window_handle = window.window_handle().ok()?;
        let display_handle = window.display_handle().ok()?;
        let RawWindowHandle::Wayland(window_handle) = window_handle.as_raw() else {
            return None;
        };
        let RawDisplayHandle::Wayland(display_handle) = display_handle.as_raw() else {
            return None;
        };
        Some((
            display_handle.display.as_ptr() as usize,
            window_handle.surface.as_ptr() as usize,
        ))
    }

    /// Attach a guest backend to winit's live `wl_display`, bind the
    /// idle-inhibit manager, and adopt the app's surface.
    fn build_wayland(display: usize, surface: usize) -> Option<Wayland> {
        // SAFETY: the winit window keeps this `wl_display` alive for the whole
        // process, and guest mode never closes it.
        let backend = unsafe { Backend::from_foreign_display(display as *mut _) };
        let connection = Connection::from_backend(backend);
        let mut queue = connection.new_event_queue::<Init>();

        let registry = connection.display().get_registry(&queue.handle(), ());
        let mut state = Init::default();
        queue.roundtrip(&mut state).ok()?;

        let (name, version) = state.manager?;
        let manager = registry.bind::<ZwpIdleInhibitManagerV1, _, _>(
            name,
            version.min(1),
            &queue.handle(),
            (),
        );
        let surface = adopt_surface(&connection, surface)?;

        Some(Wayland {
            connection,
            queue,
            manager,
            surface,
        })
    }

    fn adopt_surface(connection: &Connection, surface: usize) -> Option<wl_surface::WlSurface> {
        // SAFETY: this is winit's live `wl_surface`; it is only passed as a
        // request argument and never dispatched or destroyed by us.
        let id = unsafe {
            ObjectId::from_ptr(
                <wl_surface::WlSurface as Proxy>::interface(),
                surface as *mut _,
            )
        }
        .ok()?;
        wl_surface::WlSurface::from_id(connection, id).ok()
    }

    #[derive(Default)]
    struct Init {
        manager: Option<(u32, u32)>,
    }

    impl Dispatch<wl_registry::WlRegistry, ()> for Init {
        fn event(
            state: &mut Self,
            _registry: &wl_registry::WlRegistry,
            event: wl_registry::Event,
            _data: &(),
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
        ) {
            if let wl_registry::Event::Global {
                name,
                interface,
                version,
            } = event
                && interface == "zwp_idle_inhibit_manager_v1"
            {
                state.manager = Some((name, version));
            }
        }
    }

    impl Dispatch<ZwpIdleInhibitManagerV1, ()> for Init {
        fn event(
            _state: &mut Self,
            _proxy: &ZwpIdleInhibitManagerV1,
            _event: <ZwpIdleInhibitManagerV1 as Proxy>::Event,
            _data: &(),
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwpIdleInhibitorV1, ()> for Init {
        fn event(
            _state: &mut Self,
            _proxy: &ZwpIdleInhibitorV1,
            _event: <ZwpIdleInhibitorV1 as Proxy>::Event,
            _data: &(),
            _conn: &Connection,
            _qh: &QueueHandle<Self>,
        ) {
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use slint::winit_030::winit::window::Window;

    /// A live idle inhibitor. Unused off Linux, where `keepawake` is the only
    /// sleep inhibitor.
    pub struct Inhibitor;

    /// Capture the window's Wayland handles. A no-op off Linux.
    pub fn capture(_window: &Window) {}

    /// Arm an idle inhibitor. Always `None` off Linux.
    pub fn arm() -> Option<Inhibitor> {
        None
    }
}

pub use imp::{Inhibitor, arm, capture};
