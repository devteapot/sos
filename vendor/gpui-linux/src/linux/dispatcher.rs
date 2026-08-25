use calloop::{
    EventLoop, PostAction,
    channel::{self, Sender},
    timer::TimeoutAction,
};
use util::ResultExt;

use std::{
    mem::MaybeUninit,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use gpui::{
    GLOBAL_THREAD_TIMINGS, PlatformDispatcher, Priority, PriorityQueueReceiver,
    PriorityQueueSender, RunnableVariant, TaskTiming, ThreadTaskTimings, profiler,
};

const MAX_MAIN_TASKS_PER_DISPATCH: usize = 64;

struct TimerAfter {
    duration: Duration,
    runnable: RunnableVariant,
}

pub(crate) struct LinuxDispatcher {
    main_sender: PriorityQueueCalloopSender<RunnableVariant>,
    timer_sender: Sender<TimerAfter>,
    background_sender: PriorityQueueSender<RunnableVariant>,
    _background_threads: Vec<thread::JoinHandle<()>>,
    main_thread_id: thread::ThreadId,
}

const MIN_THREADS: usize = 2;

impl LinuxDispatcher {
    pub fn new(main_sender: PriorityQueueCalloopSender<RunnableVariant>) -> Self {
        let (background_sender, background_receiver) = PriorityQueueReceiver::new();
        let thread_count =
            std::thread::available_parallelism().map_or(MIN_THREADS, |i| i.get().max(MIN_THREADS));

        let mut background_threads = (0..thread_count)
            .map(|i| {
                let receiver: PriorityQueueReceiver<RunnableVariant> = background_receiver.clone();
                std::thread::Builder::new()
                    .name(format!("Worker-{i}"))
                    .spawn(move || {
                        for runnable in receiver.iter() {
                            let start = Instant::now();

                            let location = runnable.metadata().location;
                            let mut timing = TaskTiming {
                                location,
                                start,
                                end: None,
                            };
                            profiler::add_task_timing(timing);

                            runnable.run();

                            let end = Instant::now();
                            timing.end = Some(end);
                            profiler::add_task_timing(timing);

                            log::trace!(
                                "background thread {}: ran runnable. took: {:?}",
                                i,
                                start.elapsed()
                            );
                        }
                    })
                    .unwrap()
            })
            .collect::<Vec<_>>();

        let (timer_sender, timer_channel) = calloop::channel::channel::<TimerAfter>();
        let timer_thread = std::thread::Builder::new()
            .name("Timer".to_owned())
            .spawn(move || {
                let mut event_loop: EventLoop<()> =
                    EventLoop::try_new().expect("Failed to initialize timer loop!");

                let handle = event_loop.handle();
                let timer_handle = event_loop.handle();
                handle
                    .insert_source(timer_channel, move |e, _, _| {
                        if let channel::Event::Msg(timer) = e {
                            let mut runnable = Some(timer.runnable);
                            timer_handle
                                .insert_source(
                                    calloop::timer::Timer::from_duration(timer.duration),
                                    move |_, _, _| {
                                        if let Some(runnable) = runnable.take() {
                                            let start = Instant::now();
                                            let location = runnable.metadata().location;
                                            let mut timing = TaskTiming {
                                                location,
                                                start,
                                                end: None,
                                            };
                                            profiler::add_task_timing(timing);

                                            runnable.run();
                                            let end = Instant::now();

                                            timing.end = Some(end);
                                            profiler::add_task_timing(timing);
                                        }
                                        TimeoutAction::Drop
                                    },
                                )
                                .expect("Failed to start timer");
                        }
                    })
                    .expect("Failed to start timer thread");

                event_loop.run(None, &mut (), |_| {}).log_err();
            })
            .unwrap();

        background_threads.push(timer_thread);

        Self {
            main_sender,
            timer_sender,
            background_sender,
            _background_threads: background_threads,
            main_thread_id: thread::current().id(),
        }
    }
}

impl PlatformDispatcher for LinuxDispatcher {
    fn get_all_timings(&self) -> Vec<gpui::ThreadTaskTimings> {
        let global_timings = GLOBAL_THREAD_TIMINGS.lock();
        ThreadTaskTimings::convert(&global_timings)
    }

    fn get_current_thread_timings(&self) -> gpui::ThreadTaskTimings {
        gpui::profiler::get_current_thread_task_timings()
    }

    fn is_main_thread(&self) -> bool {
        thread::current().id() == self.main_thread_id
    }

    fn dispatch(&self, runnable: RunnableVariant, priority: Priority) {
        self.background_sender
            .send(priority, runnable)
            .unwrap_or_else(|_| panic!("blocking sender returned without value"));
    }

    fn dispatch_on_main_thread(&self, runnable: RunnableVariant, priority: Priority) {
        self.main_sender
            .send(priority, runnable)
            .unwrap_or_else(|runnable| {
                // NOTE: Runnable may wrap a Future that is !Send.
                //
                // This is usually safe because we only poll it on the main thread.
                // However if the send fails, we know that:
                // 1. main_receiver has been dropped (which implies the app is shutting down)
                // 2. we are on a background thread.
                // It is not safe to drop something !Send on the wrong thread, and
                // the app will exit soon anyway, so we must forget the runnable.
                std::mem::forget(runnable);
            });
    }

    fn dispatch_after(&self, duration: Duration, runnable: RunnableVariant) {
        self.timer_sender
            .send(TimerAfter { duration, runnable })
            .ok();
    }

    fn spawn_realtime(&self, f: Box<dyn FnOnce() + Send>) {
        std::thread::spawn(move || {
            // SAFETY: always safe to call
            let thread_id = unsafe { libc::pthread_self() };

            let policy = libc::SCHED_FIFO;
            let sched_priority = 65;

            // SAFETY: all sched_param members are valid when initialized to zero.
            let mut sched_param =
                unsafe { MaybeUninit::<libc::sched_param>::zeroed().assume_init() };
            sched_param.sched_priority = sched_priority;
            // SAFETY: sched_param is a valid initialized structure
            let result = unsafe { libc::pthread_setschedparam(thread_id, policy, &sched_param) };
            if result != 0 {
                log::warn!("failed to set realtime thread priority");
            }

            f();
        });
    }
}

pub struct PriorityQueueCalloopSender<T> {
    sender: PriorityQueueSender<T>,
    ping: calloop::ping::Ping,
    pending: Arc<AtomicUsize>,
}

impl<T> PriorityQueueCalloopSender<T> {
    fn send(&self, priority: Priority, item: T) -> Result<(), gpui::queue::SendError<T>> {
        self.pending.fetch_add(1, Ordering::Release);
        let res = self.sender.send(priority, item);
        if res.is_ok() {
            self.ping.ping();
        } else {
            self.pending.fetch_sub(1, Ordering::AcqRel);
        }
        res
    }
}

impl<T> Drop for PriorityQueueCalloopSender<T> {
    fn drop(&mut self) {
        self.ping.ping();
    }
}

pub struct PriorityQueueCalloopReceiver<T> {
    receiver: PriorityQueueReceiver<T>,
    source: calloop::ping::PingSource,
    ping: calloop::ping::Ping,
    pending: Arc<AtomicUsize>,
}

impl<T> PriorityQueueCalloopReceiver<T> {
    pub fn new() -> (PriorityQueueCalloopSender<T>, Self) {
        let (ping, source) = calloop::ping::make_ping().expect("Failed to create a Ping.");

        let (tx, rx) = PriorityQueueReceiver::new();
        let pending = Arc::new(AtomicUsize::new(0));

        (
            PriorityQueueCalloopSender {
                sender: tx,
                ping: ping.clone(),
                pending: Arc::clone(&pending),
            },
            Self {
                receiver: rx,
                source,
                ping,
                pending,
            },
        )
    }
}

use calloop::channel::Event;

#[derive(Debug)]
pub struct ChannelError(calloop::ping::PingError);

impl std::fmt::Display for ChannelError {
    #[cfg_attr(feature = "nightly_coverage", coverage(off))]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl std::error::Error for ChannelError {
    #[cfg_attr(feature = "nightly_coverage", coverage(off))]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl<T> calloop::EventSource for PriorityQueueCalloopReceiver<T> {
    type Event = Event<T>;
    type Metadata = ();
    type Ret = ();
    type Error = ChannelError;

    fn process_events<F>(
        &mut self,
        readiness: calloop::Readiness,
        token: calloop::Token,
        mut callback: F,
    ) -> Result<calloop::PostAction, Self::Error>
    where
        F: FnMut(Self::Event, &mut Self::Metadata) -> Self::Ret,
    {
        let mut clear_readiness = false;
        let mut disconnected = false;

        let action = self
            .source
            .process_events(readiness, token, |(), &mut ()| {
                let mut is_empty = true;

                let receiver = self.receiver.clone();
                // Foreground tasks may schedule more foreground work (for
                // example, a continuously animating view). Bound each turn so
                // the calloop dispatcher can return to Wayland protocol and
                // input sources instead of draining a queue that never becomes
                // empty.
                for runnable in receiver.try_iter().take(MAX_MAIN_TASKS_PER_DISPATCH) {
                    match runnable {
                        Ok(r) => {
                            self.pending.fetch_sub(1, Ordering::AcqRel);
                            callback(Event::Msg(r), &mut ());
                            is_empty = false;
                        }
                        Err(_) => {
                            disconnected = true;
                        }
                    }
                }

                if disconnected {
                    callback(Event::Closed, &mut ());
                }

                if is_empty {
                    clear_readiness = true;
                }
            })
            .map_err(ChannelError)?;

        if disconnected {
            Ok(PostAction::Remove)
        } else if self.pending.load(Ordering::Acquire) == 0 {
            Ok(action)
        } else {
            // PriorityQueueReceiver::try_iter may stop before the opaque
            // priority queue is empty. Keep the calloop source readable while
            // a successful send remains unmatched by a received runnable.
            self.ping.ping();
            Ok(PostAction::Continue)
        }
    }

    fn register(
        &mut self,
        poll: &mut calloop::Poll,
        token_factory: &mut calloop::TokenFactory,
    ) -> calloop::Result<()> {
        self.source.register(poll, token_factory)
    }

    fn reregister(
        &mut self,
        poll: &mut calloop::Poll,
        token_factory: &mut calloop::TokenFactory,
    ) -> calloop::Result<()> {
        self.source.reregister(poll, token_factory)
    }

    fn unregister(&mut self, poll: &mut calloop::Poll) -> calloop::Result<()> {
        self.source.unregister(poll)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calloop_works() {
        let mut event_loop = calloop::EventLoop::try_new().unwrap();
        let handle = event_loop.handle();

        let (tx, rx) = PriorityQueueCalloopReceiver::new();

        struct Data {
            got_msg: bool,
            got_closed: bool,
        }

        let mut data = Data {
            got_msg: false,
            got_closed: false,
        };

        let _channel_token = handle
            .insert_source(rx, move |evt, &mut (), data: &mut Data| match evt {
                Event::Msg(()) => {
                    data.got_msg = true;
                }

                Event::Closed => {
                    data.got_closed = true;
                }
            })
            .unwrap();

        // nothing is sent, nothing is received
        event_loop
            .dispatch(Some(::std::time::Duration::ZERO), &mut data)
            .unwrap();

        assert!(!data.got_msg);
        assert!(!data.got_closed);
        // a message is send

        tx.send(Priority::Medium, ()).unwrap();
        event_loop
            .dispatch(Some(::std::time::Duration::ZERO), &mut data)
            .unwrap();

        assert!(data.got_msg);
        assert!(!data.got_closed);

        // the sender is dropped
        drop(tx);
        event_loop
            .dispatch(Some(::std::time::Duration::ZERO), &mut data)
            .unwrap();

        assert!(data.got_msg);
        assert!(data.got_closed);
    }

    #[test]
    fn calloop_rewakes_until_all_priority_work_is_drained() {
        let mut event_loop = calloop::EventLoop::try_new().unwrap();
        let handle = event_loop.handle();
        let (tx, rx) = PriorityQueueCalloopReceiver::new();
        let pending = Arc::clone(&tx.pending);

        handle
            .insert_source(rx, |event, &mut (), received: &mut usize| {
                if let Event::Msg(_) = event {
                    *received += 1;
                }
            })
            .unwrap();

        for index in 0..1_000 {
            let priority = match index % 3 {
                0 => Priority::High,
                1 => Priority::Medium,
                _ => Priority::Low,
            };
            tx.send(priority, index).unwrap();
        }
        let mut received = 0;
        let deadline = Instant::now() + Duration::from_secs(1);
        while received < 1_000 && Instant::now() < deadline {
            event_loop
                .dispatch(Some(Duration::from_millis(10)), &mut received)
                .unwrap();
        }

        assert_eq!(received, 1_000);
        assert_eq!(pending.load(Ordering::Acquire), 0);
    }

    #[test]
    fn calloop_yields_between_sustained_foreground_batches() {
        let mut event_loop = calloop::EventLoop::try_new().unwrap();
        let handle = event_loop.handle();
        let (tx, rx) = PriorityQueueCalloopReceiver::new();
        let tx = Arc::new(tx);
        let callback_tx = Arc::clone(&tx);
        let total = MAX_MAIN_TASKS_PER_DISPATCH * 3;

        handle
            .insert_source(rx, move |event, &mut (), received: &mut usize| {
                if let Event::Msg(_) = event {
                    *received += 1;
                    if *received < total {
                        callback_tx.send(Priority::High, *received).unwrap();
                    }
                }
            })
            .unwrap();

        tx.send(Priority::High, 0).unwrap();
        let mut received = 0;
        event_loop
            .dispatch(Some(Duration::ZERO), &mut received)
            .unwrap();

        assert_eq!(received, MAX_MAIN_TASKS_PER_DISPATCH);

        while received < total {
            event_loop
                .dispatch(Some(Duration::from_millis(10)), &mut received)
                .unwrap();
        }
        assert_eq!(tx.pending.load(Ordering::Acquire), 0);
    }
}

// running 1 test
// test linux::dispatcher::tests::tomato ... FAILED

// failures:

// ---- linux::dispatcher::tests::tomato stdout ----
// [crates/gpui/src/platform/linux/dispatcher.rs:262:9]
// returning 1 tasks to process
// [crates/gpui/src/platform/linux/dispatcher.rs:480:75] evt = Msg(
//     (),
// )
// returning 0 tasks to process

// thread 'linux::dispatcher::tests::tomato' (478301) panicked at crates/gpui/src/platform/linux/dispatcher.rs:515:9:
// assertion failed: data.got_closed
// note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
