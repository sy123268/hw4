use std::{
    cell::RefCell,
    future::Future,
    sync::{Arc, Condvar, Mutex},
    task::{Context, Poll, RawWaker, RawWakerVTable, Wake, Waker},
};

use futures::future::BoxFuture;
use scoped_tls::scoped_thread_local;
use std::collections::VecDeque;


struct Signal {
    state: Mutex<State>,
    cond: Condvar,
}

enum State {
    Empty,
    Wating,
    Notified,
}

impl Wake for Signal {
    fn wake(self: Arc<Self>) {
        self.notify();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.notify();
    }
}

impl Signal {
    fn new() -> Self {
        Self {
            state: Mutex::new(State::Empty),
            cond: Condvar::new(),
        }
    }
    fn wait(&self) {
        let mut state = self.state.lock().unwrap();
        match *state {
            State::Empty => {
                *state = State::Wating;
                while let State::Wating = *state {
                    state = self.cond.wait(state).unwrap();
                }
            }
            State::Wating => {
                panic!("cannot wait twice");
            }
            State::Notified => {
                *state = State::Empty;
            }
        }
    }
    fn notify(&self) {
        let mut state = self.state.lock().unwrap();
        match *state {
            State::Empty => {
                *state = State::Notified;
            }
            State::Wating => {
                *state = State::Empty;
                self.cond.notify_one();
            }
            State::Notified => {
                println!("already notified")
            }
        }
    }
}

fn block_on<F: Future>(future: F) -> F::Output {
    let mut fut = std::pin::pin!(future);
    let signal = Arc::new(Signal::new());
    let waker = Waker::from(signal.clone());

    let mut cx = Context::from_waker(&waker);
    // loop {
    //     if let Poll::Ready(val) = fut.as_mut().poll(&mut cx) {
    //         return val;
    //     }
    // }

    let runnable = Mutex::new(VecDeque::with_capacity(1024));
    SIGNAL.set(&signal, || {
        RUNNABLE.set(&runnable, || loop {
            if let Poll::Ready(output) = fut.as_mut().poll(&mut cx) {
                return output;
            }
            while let Some(task) = runnable.lock().unwrap().pop_front() {
                let waker = Waker::from(task.clone());
                let mut cx = Context::from_waker(&waker);
                let _ = task.future.borrow_mut().as_mut().poll(&mut cx);
            }
            signal.wait();
        })
    })
}


async fn demo() {
    let (tx, rx) = async_channel::bounded::<()>(1);
    std::thread::spawn(move || {
        // std::thread::sleep(std::time::Duration::from_secs(2));
        tx.send_blocking(()).unwrap();
    });
    let _ = rx.recv().await;
    println!("Hello, world!");
    //sleep
}


async fn demo2(tx: async_channel::Sender<()>) {
    println!("Hello, world!2");
    tx.send(()).await.unwrap();
}

struct Task {
    future: RefCell<BoxFuture<'static, ()>>,
    signal: Arc<Signal>,
}
unsafe impl Send for Task {}
unsafe impl Sync for Task {}

impl Wake for Task {
    fn wake(self: Arc<Self>) {
        RUNNABLE.with(|runnable| {
            runnable.lock().unwrap().push_back(self.clone());
            self.signal.notify();
        })
    }
}

scoped_thread_local!(static RUNNABLE: Mutex<VecDeque<Arc<Task>>>);
scoped_thread_local!(static SIGNAL: Arc<Signal>);

fn main() {

    block_on(demo());

}
