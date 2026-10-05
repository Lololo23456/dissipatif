//! Binaire jouable. Pour l'instant : ouvre une fenêtre vide.
//! Prochaine étape : initialiser wgpu dans `render` et y afficher la première parcelle.

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

#[derive(Default)]
struct App {
    window: Option<Window>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attributes = Window::default_attributes().with_title("Dissipatif");
            let window = event_loop
                .create_window(attributes)
                .expect("création de la fenêtre");
            self.window = Some(window);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        if let WindowEvent::CloseRequested = event {
            event_loop.exit();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("création de la boucle d'événements");
    let mut app = App::default();
    event_loop.run_app(&mut app).expect("exécution de la boucle d'événements");
}
