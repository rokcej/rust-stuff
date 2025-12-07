use rand::Rng;
use rand_distr::{Distribution, Normal};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

// Hotkey for turning auto clicker on/off
const RUNNING_TOGGLE_KEY: rdev::Key = rdev::Key::F8;

// Time between clicks
const CLICK_INTERVAL_MEAN_MS: f64 = 1200.0;
const CLICK_INTERVAL_SD_MS: f64 = CLICK_INTERVAL_MEAN_MS / 6.0;

// Time between click down & up
const HOLD_DURATION_MEAN_MS: f64 = 85.0;
const HOLD_DURATION_SD_MS: f64 = HOLD_DURATION_MEAN_MS / 6.0;

// Stop after moving mouse more than N pixels
const MOVE_STOP_DISTANCE_PX: f64 = 16.0;

struct Data {
    is_running: AtomicBool,
    initial_mouse_pos: Mutex<Option<(f64, f64)>>,
}

impl Data {
    fn new() -> Self {
        return Self {
            is_running: AtomicBool::new(false),
            initial_mouse_pos: Mutex::new(None),
        };
    }

    fn get_running(&self) -> bool {
        return self.is_running.load(Ordering::SeqCst);
    }

    fn set_running(&self, new_running: bool, reason: &str) {
        if new_running == self.get_running() {
            return;
        }
        if new_running {
            *self.initial_mouse_pos.lock().unwrap() = None;
        }
        self.is_running.store(new_running, Ordering::SeqCst);

        let new_running_str = if new_running { "ON " } else { "OFF" };
        println!("Auto clicker: {new_running_str} ({reason})",);
    }
}

fn main() {
    println!("==== AUTO CLICKER ====");
    println!("Toggle on/off hotkey: {RUNNING_TOGGLE_KEY:?}");
    println!("Click interval:       {CLICK_INTERVAL_MEAN_MS} ms (SD = {CLICK_INTERVAL_SD_MS} ms)");
    println!("Hold duration:        {HOLD_DURATION_MEAN_MS} ms (SD = {HOLD_DURATION_SD_MS} ms)");
    println!("Mouse move threshold: {MOVE_STOP_DISTANCE_PX} px");
    println!("");
    println!("==== LOG ====");

    let data = Arc::new(Data::new());

    {
        // Spawn clicker thread
        let data = Arc::clone(&data);
        thread::spawn(move || clicker_thread(data));
    }

    {
        // Spawn listener thread
        let data = Arc::clone(&data);
        thread::spawn(move || listener_thread(data));
    }

    loop {
        // Keep main thread alive forever
        thread::park();
    }
}

fn clicker_thread(data: Arc<Data>) {
    let mut rng = rand::rng();
    let click_distribution = Normal::new(CLICK_INTERVAL_MEAN_MS, CLICK_INTERVAL_SD_MS)
        .expect("Invalid normal distribution");
    let hold_distribution = Normal::new(HOLD_DURATION_MEAN_MS, HOLD_DURATION_SD_MS)
        .expect("Invalid normal distribution");

    loop {
        if !data.get_running() {
            // Not running, wait a bit
            thread::sleep(Duration::from_millis(250));
            continue;
        }

        let click_interval_ms = sample_positive(&click_distribution, &mut rng);
        let hold_duration_ms = sample_positive(&hold_distribution, &mut rng);

        send_event(&rdev::EventType::ButtonPress(rdev::Button::Left));
        adaptive_wait(Duration::from_secs_f64(hold_duration_ms * 1e-3));
        send_event(&rdev::EventType::ButtonRelease(rdev::Button::Left));
        adaptive_wait(Duration::from_secs_f64(click_interval_ms * 1e-3));
    }
}

fn sample_positive<R: Rng>(distribution: &Normal<f64>, rng: &mut R) -> f64 {
    for _ in 0..10 {
        let value = distribution.sample(rng);
        if value > 0.0 {
            return value;
        }
    }
    eprintln!("Error generating a positive sample");
    return distribution.mean();
}

fn send_event(event_type: &rdev::EventType) {
    if let Err(error) = rdev::simulate(event_type) {
        eprintln!("Error sending event {event_type:?}: {error}");
    }

    // Let the OS process the event
    thread::sleep(Duration::from_millis(1));
}

fn adaptive_wait(duration: Duration) {
    let end = Instant::now() + duration;
    loop {
        let now = Instant::now();
        if now >= end {
            break;
        }
        let remaining = end - now;
        thread::sleep(remaining / 4);
    }
}

fn listener_thread(data: Arc<Data>) {
    // Blocking function
    let result = rdev::listen(move |event| {
        handle_event(event, &data);
    });

    if let Err(error) = result {
        eprintln!("Error listening to events: {:?}", error);
    }
}

fn handle_event(event: rdev::Event, data: &Data) {
    match event.event_type {
        rdev::EventType::KeyPress(RUNNING_TOGGLE_KEY) => {
            let new_running = !data.get_running();
            data.set_running(new_running, "hotkey pressed");
        }
        rdev::EventType::MouseMove { x, y } => {
            if !data.get_running() {
                return;
            }

            let mut pos_guard = data.initial_mouse_pos.lock().unwrap();
            if let Some((initial_x, initial_y)) = *pos_guard {
                let dx = x - initial_x;
                let dy = y - initial_y;
                let dist_sq = dx * dx + dy * dy;

                if dist_sq > MOVE_STOP_DISTANCE_PX * MOVE_STOP_DISTANCE_PX {
                    data.set_running(false, "mouse moved");
                }
            } else {
                *pos_guard = Some((x, y));
            }
        }
        _ => {}
    }
}
