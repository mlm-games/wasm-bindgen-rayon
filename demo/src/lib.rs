use hsl::HSL;
use js_sys::Date;
use num_complex::Complex64;
use rand::Rng;
use rayon::prelude::*;
use wasm_bindgen::{prelude::*, Clamped};
use web_sys::{CanvasRenderingContext2d, ImageData};

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

fn draw(canvas_id: &str, time_id: &str, parallel: bool) -> Result<(), JsValue> {
    let document = web_sys::window().unwrap().document().unwrap();
    let canvas = document
        .get_element_by_id(canvas_id)
        .unwrap()
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    let width = canvas.width();
    let height = canvas.height();
    let ctx = canvas
        .get_context("2d")?
        .unwrap()
        .dyn_into::<CanvasRenderingContext2d>()?;

    if parallel {
        let n = web_sys::window().unwrap().navigator().hardware_concurrency() as usize;
        let threads = n.max(1).min(32);
        let _ = wasm_bindgen_rayon::init_thread_pool(threads);
    }

    let gen = Generator::new(width, height, 1000);
    let start = Date::now();

    let pixels = if parallel {
        gen.render_par()
    } else {
        gen.render_seq()
    };

    let elapsed = Date::now() - start;

    let img_data = ImageData::new_with_u8_clamped_array(Clamped(&pixels), width)?;
    ctx.put_image_data(&img_data, 0.0, 0.0)?;

    let output = document.get_element_by_id(time_id).unwrap();
    output.set_text_content(Some(&format!("{:.2} ms", elapsed)));

    Ok(())
}

#[wasm_bindgen(start)]
fn start() -> Result<(), JsValue> {
    let document = web_sys::window().unwrap().document().unwrap();

    let single_btn = document
        .get_element_by_id("singleThread")
        .unwrap()
        .dyn_into::<web_sys::HtmlButtonElement>()?;
    single_btn.remove_attribute("disabled")?;

    let multi_btn = document
        .get_element_by_id("multiThread")
        .unwrap()
        .dyn_into::<web_sys::HtmlButtonElement>()?;

    let is_isolated = js_sys::Reflect::get(
        &web_sys::window().unwrap(),
        &"crossOriginIsolated".into(),
    )
    .ok()
    .and_then(|v| v.as_bool())
    .unwrap_or(false);

    if !is_isolated {
        multi_btn.set_value("Multi-threaded (requires COOP/COEP headers)");
    } else {
        multi_btn.remove_attribute("disabled")?;
    }

    let cb = Closure::wrap(Box::new(move || {
        let _ = draw("canvas", "time", false);
    }) as Box<dyn FnMut()>);
    single_btn.add_event_listener_with_callback("click", cb.as_ref().unchecked_ref())?;
    cb.forget();

    let cb2 = Closure::wrap(Box::new(move || {
        let _ = draw("canvas", "time", true);
    }) as Box<dyn FnMut()>);
    multi_btn.add_event_listener_with_callback("click", cb2.as_ref().unchecked_ref())?;
    cb2.forget();

    Ok(())
}
