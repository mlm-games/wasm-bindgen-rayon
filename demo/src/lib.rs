use std::sync::atomic::{AtomicBool, Ordering};

use hsl::HSL;
use num_complex::Complex64;
use rand::Rng;
use rayon::prelude::*;
use repose_core::*;
use repose_material::material3::{Button, ButtonConfig};
use repose_platform::RenderContext;
use repose_ui::{TextStyle, *};
use wasm_bindgen::prelude::*;

static POOL_READY: AtomicBool = AtomicBool::new(false);

type RGBA = [u8; 4];

struct Generator {
    width: u32,
    height: u32,
    palette: Box<[RGBA]>,
}

impl Generator {
    fn new(width: u32, height: u32, max_iterations: u32) -> Self {
        let max_iterations = max_iterations.max(1);
        let mut rng = rand::thread_rng();
        Self {
            width,
            height,
            palette: (0..=max_iterations)
                .map(|_| {
                    let (r, g, b) = HSL {
                        h: rng.gen_range(0.0..360.0),
                        s: 0.5,
                        l: 0.6,
                    }
                    .to_rgb();
                    [r, g, b, 255]
                })
                .collect(),
        }
    }

    fn get_color(&self, x: u32, y: u32) -> &RGBA {
        let c = Complex64::new(
            (x as f64 - self.width as f64 / 2.0) * 4.0 / self.width as f64,
            (y as f64 - self.height as f64 / 2.0) * 4.0 / self.height as f64,
        );
        let mut z = Complex64::new(0.0, 0.0);
        let max = self.palette.len() - 1;
        let mut i = 0;
        while i < max && z.norm_sqr() < 4.0 {
            z = z * z + c;
            i += 1;
        }
        &self.palette[i]
    }

    fn render_par(&self) -> Vec<u8> {
        let row_len = self.width as usize * 4;
        let mut result = vec![0u8; row_len * self.height as usize];
        result
            .par_chunks_mut(row_len)
            .enumerate()
            .for_each(|(y, row)| {
                for x in 0..self.width {
                    let color = self.get_color(x, y as u32);
                    let offset = x as usize * 4;
                    row[offset..offset + 4].copy_from_slice(color);
                }
            });
        result
    }

    fn render_seq(&self) -> Vec<u8> {
        let mut result = vec![0u8; self.width as usize * self.height as usize * 4];
        for y in 0..self.height {
            for x in 0..self.width {
                let color = self.get_color(x, y);
                let offset = (y * self.width + x) as usize * 4;
                result[offset..offset + 4].copy_from_slice(color);
            }
        }
        result
    }
}

const WIDTH: u32 = 700;
const HEIGHT: u32 = 700;
const MAX_ITER: u32 = 1000;

fn app(s: &mut Scheduler, rc: &RenderContext) -> View {
    let th = theme();

    let img_handle = remember_state_with_key("img", || rc.alloc_image_handle());
    let timing_text = remember_state_with_key("timing", String::new);

    let img_handle_val = *img_handle.borrow();

    let on_single = {
        let rc = rc.clone();
        let img = img_handle.clone();
        let timing = timing_text.clone();
        move || {
            let img_gen = Generator::new(WIDTH, HEIGHT, MAX_ITER);
            let start = js_sys::Date::now();
            let pixels = img_gen.render_seq();
            let elapsed = js_sys::Date::now() - start;
            rc.set_image_rgba8(*img.borrow(), WIDTH, HEIGHT, pixels, true);
            *timing.borrow_mut() = format!("{:.2} ms (single thread)", elapsed);
            request_frame();
        }
    };

    let on_multi = {
        let rc = rc.clone();
        let img = img_handle.clone();
        let timing = timing_text.clone();
        move || {
            let image_gen = Generator::new(WIDTH, HEIGHT, MAX_ITER);
            let start = js_sys::Date::now();
            let pixels = image_gen.render_par();
            let elapsed = js_sys::Date::now() - start;
            rc.set_image_rgba8(*img.borrow(), WIDTH, HEIGHT, pixels, true);
            *timing.borrow_mut() = format!("{:.2} ms (multi thread)", elapsed);
            request_frame();
        }
    };

    let timing_str = timing_text.borrow().clone();
    let multi_enabled = POOL_READY.load(Ordering::Acquire);

    Column(
        Modifier::new()
            .fill_max_size()
            .background(th.background)
            .padding(16.0)
            .align_items(AlignItems::CENTER),
    )
    .child((
        Text("Mandelbrot Fractal")
            .size(24.0)
            .color(th.on_background)
            .modifier(Modifier::new().padding(8.0)),
        Text("Powered by wasm-bindgen-rayon")
            .size(14.0)
            .color(th.on_surface_variant)
            .modifier(Modifier::new().padding(16.0)),
        Row(Modifier::new().padding(12.0)).child((
            Button(Modifier::new(), on_single, ButtonConfig::default(), || {
                Text("Single thread").modifier(Modifier::new().padding_values(PaddingValues {
                    left: 16.0,
                    right: 16.0,
                    top: 8.0,
                    bottom: 8.0,
                }))
            }),
            if multi_enabled {
                Button(
                    Modifier::new().padding_values(PaddingValues {
                        left: 8.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }),
                    on_multi,
                    ButtonConfig::default(),
                    || {
                        Text("All threads").modifier(Modifier::new().padding_values(
                            PaddingValues {
                                left: 16.0,
                                right: 16.0,
                                top: 8.0,
                                bottom: 8.0,
                            },
                        ))
                    },
                )
            } else {
                Button(
                    Modifier::new().padding_values(PaddingValues {
                        left: 8.0,
                        right: 0.0,
                        top: 0.0,
                        bottom: 0.0,
                    }),
                    || {},
                    ButtonConfig {
                        enabled: false,
                        ..Default::default()
                    },
                    || {
                        Text("Initializing...").modifier(Modifier::new().padding_values(
                            PaddingValues {
                                left: 16.0,
                                right: 16.0,
                                top: 8.0,
                                bottom: 8.0,
                            },
                        ))
                    },
                )
            },
        )),
        scope!("timing", s, [timing_str], {
            if timing_str.is_empty() {
                Spacer()
            } else {
                Text(&timing_str).size(14.0).color(th.on_surface_variant)
            }
        }),
        scope!("image", s, [img_handle_val], {
            Image(
                Modifier::new()
                    .size(WIDTH as f32, HEIGHT as f32)
                    .padding(12.0),
                img_handle_val,
            )
            .image_fit(ImageFit::Contain)
        }),
    ))
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let n = web_sys::window()
        .map(|w: web_sys::Window| w.navigator().hardware_concurrency() as usize)
        .unwrap_or(1)
        .max(1)
        .min(32);

    let promise = wasm_bindgen_rayon::init_thread_pool(n);
    wasm_bindgen_futures::spawn_local(async move {
        if wasm_bindgen_futures::JsFuture::from(promise).await.is_ok() {
            POOL_READY.store(true, Ordering::SeqCst);
            request_frame();
        }
    });

    repose_platform::web::run_web_app(app, repose_platform::web::WebOptions::new(None))
}
