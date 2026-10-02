use std::num::NonZeroU32;
use std::rc::Rc;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

const FONT: &[u8] = include_bytes!("../../../../apps/gmnb/assets/fonts/Outfit-Variable.ttf");
const KEYS: [&str; 24] = ["%", "CE", "C", "⌫", "1/x", "x²", "√x", "÷", "7", "8", "9", "×", "4", "5", "6", "−", "1", "2", "3", "+", "±", "0", ".", "="];

struct App {
    win: Option<(Rc<Window>, softbuffer::Surface<Rc<Window>, Rc<Window>>)>,
    font: fontdue::Font,
}

fn rrect(x: f32, y: f32, w: f32, h: f32, r: f32) -> tiny_skia::Path {
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish().unwrap()
}

fn text(pm: &mut Pixmap, font: &fontdue::Font, s: &str, size: f32, cx: f32, cy: f32, rgb: (u8, u8, u8)) {
    let glyphs: Vec<_> = s.chars().map(|c| font.rasterize(c, size)).collect();
    let w: f32 = glyphs.iter().map(|(m, _)| m.advance_width).sum();
    let mut x = cx - w / 2.0;
    let base = cy + size * 0.35;
    let (pw, ph) = (pm.width() as i32, pm.height() as i32);
    let data = pm.data_mut();
    for (m, bmp) in glyphs {
        let gx = (x + m.xmin as f32) as i32;
        let gy = (base - m.height as f32 - m.ymin as f32) as i32;
        for row in 0..m.height as i32 {
            for col in 0..m.width as i32 {
                let (px, py) = (gx + col, gy + row);
                if px < 0 || py < 0 || px >= pw || py >= ph { continue; }
                let a = bmp[(row * m.width as i32 + col) as usize] as u32;
                if a == 0 { continue; }
                let i = ((py * pw + px) * 4) as usize;
                for (k, c) in [rgb.0, rgb.1, rgb.2].into_iter().enumerate() {
                    data[i + k] = ((c as u32 * a + data[i + k] as u32 * (255 - a)) / 255) as u8;
                }
            }
        }
        x += m.advance_width;
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        let attrs = Window::default_attributes().with_title("raw").with_inner_size(winit::dpi::LogicalSize::new(760, 700));
        let w = Rc::new(el.create_window(attrs).unwrap());
        let ctx = softbuffer::Context::new(w.clone()).unwrap();
        let s = softbuffer::Surface::new(&ctx, w.clone()).unwrap();
        self.win = Some((w, s));
    }
    fn window_event(&mut self, el: &ActiveEventLoop, _: WindowId, ev: WindowEvent) {
        match ev {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(_) => self.win.as_ref().unwrap().0.request_redraw(),
            WindowEvent::RedrawRequested => {
                let (w, s) = self.win.as_mut().unwrap();
                let size = w.inner_size();
                let (Some(wd), Some(ht)) = (NonZeroU32::new(size.width), NonZeroU32::new(size.height)) else { return };
                s.resize(wd, ht).unwrap();
                let mut pm = Pixmap::new(size.width, size.height).unwrap();
                pm.fill(Color::from_rgba8(246, 246, 248, 255));
                let mut paint = Paint::default();
                paint.anti_alias = true;
                text(&mut pm, &self.font, "1,234,567.89", 64.0, size.width as f32 / 2.0, 120.0, (20, 20, 24));
                let (cols, rows) = (4, 6);
                let (gx, gy) = (16.0, 200.0);
                let kw = (size.width as f32 - 32.0 - 3.0 * 6.0) / cols as f32;
                let kh = (size.height as f32 - gy - 16.0 - 5.0 * 6.0) / rows as f32;
                for (i, k) in KEYS.iter().enumerate() {
                    let (c, r) = ((i % cols) as f32, (i / cols) as f32);
                    let (x, y) = (gx + c * (kw + 6.0), gy + r * (kh + 6.0));
                    let eq = *k == "=";
                    paint.set_color(if eq { Color::from_rgba8(60, 90, 220, 255) } else { Color::from_rgba8(255, 255, 255, 255) });
                    pm.fill_path(&rrect(x, y, kw, kh, 8.0), &paint, FillRule::Winding, Transform::identity(), None);
                    text(&mut pm, &self.font, k, 22.0, x + kw / 2.0, y + kh / 2.0, if eq { (255, 255, 255) } else { (20, 20, 24) });
                }
                let mut buf = s.buffer_mut().unwrap();
                for (d, p) in buf.iter_mut().zip(pm.data().chunks_exact(4)) {
                    *d = (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32;
                }
                buf.present().unwrap();
            }
            _ => {}
        }
    }
}

fn main() {
    let el = EventLoop::new().unwrap();
    let font = fontdue::Font::from_bytes(FONT, fontdue::FontSettings::default()).unwrap();
    el.run_app(&mut App { win: None, font }).unwrap();
}
