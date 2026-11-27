use crate::backend::{render_preview_png, StudioPaths};
use image::imageops::FilterType;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread;

const MAX_PREVIEW_WIDTH: u32 = 560;
const MAX_UPLOADS_PER_FRAME: usize = 8;

#[derive(Default)]
pub struct PrerenderProgress {
    pub running: AtomicBool,
    pub done: AtomicUsize,
    pub total: AtomicUsize,
    pub failed: AtomicUsize,
}

impl PrerenderProgress {
    pub fn snapshot(&self) -> (bool, usize, usize, usize) {
        (
            self.running.load(Ordering::Relaxed),
            self.done.load(Ordering::Relaxed),
            self.total.load(Ordering::Relaxed),
            self.failed.load(Ordering::Relaxed),
        )
    }
}

pub enum PreviewStatus {
    Loading,
    Ready(egui::TextureHandle),
    Failed,
}

struct LoadedThumb {
    name: String,
    image: Option<egui::ColorImage>,
}

struct WorkQueue {
    items: Mutex<VecDeque<String>>,
    cv: Condvar,
}

impl WorkQueue {
    fn new() -> Self {
        Self {
            items: Mutex::new(VecDeque::new()),
            cv: Condvar::new(),
        }
    }

    fn push(&self, item: String) {
        if let Ok(mut q) = self.items.lock() {
            q.push_back(item);
            self.cv.notify_one();
        }
    }

    fn pop_blocking(&self) -> String {
        let mut q = self.items.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(v) = q.pop_front() {
                return v;
            }
            q = self.cv.wait(q).unwrap_or_else(|e| e.into_inner());
        }
    }

    fn clear(&self) {
        if let Ok(mut q) = self.items.lock() {
            q.clear();
        }
    }
}

fn decode_png_scaled(path: &Path) -> Option<egui::ColorImage> {
    let img = image::open(path).ok()?;
    let rgba = if img.width() > MAX_PREVIEW_WIDTH {
        let new_h = ((img.height() as u64 * MAX_PREVIEW_WIDTH as u64) / img.width().max(1) as u64)
            .max(1) as u32;
        img.resize_exact(MAX_PREVIEW_WIDTH, new_h, FilterType::Triangle)
            .to_rgba8()
    } else {
        img.to_rgba8()
    };
    let size = [rgba.width() as usize, rgba.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(
        size,
        rgba.as_raw(),
    ))
}

pub struct PreviewLoader {
    paths: StudioPaths,
    textures: HashMap<String, PreviewStatus>,
    disk_queue: Arc<WorkQueue>,
    render_queue: Arc<WorkQueue>,
    in_flight: HashSet<String>,
    rx: mpsc::Receiver<LoadedThumb>,
    tx: mpsc::Sender<LoadedThumb>,
    repaint_ctx: Arc<Mutex<Option<egui::Context>>>,
    workers_started: bool,
    pub prerender: Arc<PrerenderProgress>,
}

impl PreviewLoader {
    pub fn new(paths: StudioPaths) -> Self {
        let (tx, rx) = mpsc::channel();
        Self {
            paths,
            textures: HashMap::new(),
            disk_queue: Arc::new(WorkQueue::new()),
            render_queue: Arc::new(WorkQueue::new()),
            in_flight: HashSet::new(),
            rx,
            tx,
            repaint_ctx: Arc::new(Mutex::new(None)),
            workers_started: false,
            prerender: Arc::new(PrerenderProgress::default()),
        }
    }

    fn ensure_workers(&mut self, ctx: &egui::Context) {
        if let Ok(mut guard) = self.repaint_ctx.lock() {
            *guard = Some(ctx.clone());
        }
        if self.workers_started {
            return;
        }
        self.workers_started = true;

        // 2 fast disk PNG decoder workers
        for _ in 0..2 {
            let q = Arc::clone(&self.disk_queue);
            let tx = self.tx.clone();
            let paths = self.paths.clone();
            let rctx = Arc::clone(&self.repaint_ctx);
            thread::spawn(move || loop {
                let name = q.pop_blocking();
                let png = paths.cache_dir.join(format!("{name}.png"));
                let image = decode_png_scaled(&png);
                let _ = tx.send(LoadedThumb { name, image });
                if let Ok(guard) = rctx.lock() {
                    if let Some(c) = guard.as_ref() {
                        c.request_repaint();
                    }
                }
            });
        }

        // 4 on-demand render workers (literofi.exe --dump-png)
        for _ in 0..4 {
            let q = Arc::clone(&self.render_queue);
            let tx = self.tx.clone();
            let paths = self.paths.clone();
            let rctx = Arc::clone(&self.repaint_ctx);
            thread::spawn(move || loop {
                let name = q.pop_blocking();
                let image =
                    render_preview_png(&paths, &name, false).and_then(|p| decode_png_scaled(&p));
                let _ = tx.send(LoadedThumb { name, image });
                if let Ok(guard) = rctx.lock() {
                    if let Some(c) = guard.as_ref() {
                        c.request_repaint();
                    }
                }
            });
        }
    }

    pub fn get_texture(&self, name: &str) -> Option<&egui::TextureHandle> {
        match self.textures.get(name) {
            Some(PreviewStatus::Ready(tex)) => Some(tex),
            _ => None,
        }
    }

    pub fn is_failed(&self, name: &str) -> bool {
        matches!(self.textures.get(name), Some(PreviewStatus::Failed))
    }

    pub fn request_preview(&mut self, name: &str, ctx: &egui::Context) {
        if self.textures.contains_key(name) || self.in_flight.contains(name) {
            return;
        }
        self.ensure_workers(ctx);
        self.textures
            .insert(name.to_string(), PreviewStatus::Loading);
        self.in_flight.insert(name.to_string());

        let png = self.paths.cache_dir.join(format!("{name}.png"));
        let on_disk = std::fs::metadata(&png)
            .map(|m| m.is_file() && m.len() > 256)
            .unwrap_or(false);
        if on_disk {
            self.disk_queue.push(name.to_string());
        } else {
            self.render_queue.push(name.to_string());
        }
    }

    pub fn pump_uploads(&mut self, ctx: &egui::Context) -> bool {
        let mut uploaded = 0usize;
        while uploaded < MAX_UPLOADS_PER_FRAME {
            match self.rx.try_recv() {
                Ok(item) => {
                    self.in_flight.remove(&item.name);
                    if let Some(img) = item.image {
                        let tex = ctx.load_texture(
                            format!("theme_preview:{}", item.name),
                            img,
                            egui::TextureOptions::LINEAR,
                        );
                        self.textures.insert(item.name, PreviewStatus::Ready(tex));
                    } else {
                        self.textures.insert(item.name, PreviewStatus::Failed);
                    }
                    uploaded += 1;
                }
                Err(_) => break,
            }
        }
        if uploaded >= MAX_UPLOADS_PER_FRAME {
            ctx.request_repaint();
            return true;
        }
        uploaded > 0
    }

    pub fn invalidate_one(&mut self, name: &str) {
        self.textures.remove(name);
        self.in_flight.remove(name);
        let png = self.paths.cache_dir.join(format!("{name}.png"));
        if png.exists() {
            let _ = std::fs::remove_file(png);
        }
    }

    pub fn clear_all(&mut self) {
        self.disk_queue.clear();
        self.render_queue.clear();
        self.textures.clear();
        self.in_flight.clear();
    }

    pub fn force_rerender_async(&mut self, name: String, ctx: &egui::Context) {
        self.ensure_workers(ctx);
        self.invalidate_one(&name);
        self.textures.insert(name.clone(), PreviewStatus::Loading);
        self.in_flight.insert(name.clone());
        let tx = self.tx.clone();
        let paths = self.paths.clone();
        let c = ctx.clone();
        thread::spawn(move || {
            let image = render_preview_png(&paths, &name, true).and_then(|p| decode_png_scaled(&p));
            let _ = tx.send(LoadedThumb { name, image });
            c.request_repaint();
        });
    }

    pub fn start_batch_prerender(
        &mut self,
        targets: Vec<String>,
        force: bool,
        ctx: &egui::Context,
    ) -> bool {
        self.ensure_workers(ctx);
        if self
            .prerender
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return false;
        }
        let total = targets.len();
        self.prerender.done.store(0, Ordering::Relaxed);
        self.prerender.failed.store(0, Ordering::Relaxed);
        self.prerender.total.store(total, Ordering::Relaxed);

        if total == 0 {
            self.prerender.running.store(false, Ordering::SeqCst);
            return true;
        }

        let queue = Arc::new(Mutex::new(targets));
        let prog = Arc::clone(&self.prerender);
        let paths = self.paths.clone();
        let tx = self.tx.clone();
        let repaint = ctx.clone();

        thread::spawn(move || {
            let mut handles = Vec::with_capacity(4);
            for _ in 0..4 {
                let q = Arc::clone(&queue);
                let pr = Arc::clone(&prog);
                let p = paths.clone();
                let t = tx.clone();
                let c = repaint.clone();
                handles.push(thread::spawn(move || loop {
                    let next = {
                        let mut g = q.lock().unwrap_or_else(|e| e.into_inner());
                        g.pop()
                    };
                    let Some(name) = next else { break };
                    let rendered = render_preview_png(&p, &name, force)
                        .and_then(|png| decode_png_scaled(&png));
                    if rendered.is_none() {
                        pr.failed.fetch_add(1, Ordering::Relaxed);
                    }
                    let _ = t.send(LoadedThumb {
                        name,
                        image: rendered,
                    });
                    pr.done.fetch_add(1, Ordering::Relaxed);
                    c.request_repaint();
                }));
            }
            for h in handles {
                let _ = h.join();
            }
            prog.running.store(false, Ordering::SeqCst);
            repaint.request_repaint();
        });
        true
    }

    pub fn has_pending(&self) -> bool {
        !self.in_flight.is_empty() || self.prerender.running.load(Ordering::Relaxed)
    }
}
