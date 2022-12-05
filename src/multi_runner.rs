use std::{
    collections::HashMap,
    sync::mpsc::{channel, Receiver, Sender},
    thread::{spawn, JoinHandle},
};

pub type RunResult<E> = Result<(), (String, E)>;

pub struct ParRunner<E: Send + 'static, P: ProgressListener> {
    max_threads: usize,
    handles: HashMap<usize, JoinHandle<()>>,
    names: HashMap<usize, String>,

    receiver: Receiver<(usize, Result<(), E>)>,
    sender: Sender<(usize, Result<(), E>)>,

    progress: P,
}

pub trait ProgressListener {
    fn on_start(&mut self, name: &str);
    fn on_finish(&mut self, name: &str);
}

impl<E: Send + 'static, P: ProgressListener> ParRunner<E, P> {
    #[allow(dead_code)]
    pub fn new(p: P) -> Self {
        let parallel = num_cpus::get();
        eprintln!("Running up to {parallel} tasks in parallel");
        Self::with_parallel(parallel, p)
    }

    #[allow(dead_code)]
    pub fn with_parallel(max_threads: usize, progress: P) -> Self {
        let (sender, receiver) = channel();

        ParRunner {
            max_threads,
            handles: Default::default(),
            names: Default::default(),
            sender,
            receiver,
            progress,
        }
    }

    pub fn run(
        &mut self,
        name: &str,
        f: impl FnOnce() -> Result<(), E> + Send + 'static,
    ) -> RunResult<E> {
        self.check_finished()?;

        if self.handles.len() >= self.max_threads {
            self.wait_receive_one()?;
        }

        let id = (0..self.max_threads)
            .find(|n| !self.handles.contains_key(n))
            .unwrap();

        let sender = self.sender.clone();
        self.handles
            .insert(id, spawn(move || sender.send((id, f())).unwrap()));

        self.progress.on_start(&name);

        self.names.insert(id, name.to_string());

        Ok(())
    }

    fn check_finished(&mut self) -> RunResult<E> {
        while let Ok((id, r)) = self.receiver.try_recv() {
            let name = self.on_finished(id);
            if let Err(e) = r {
                return Err((name, e));
            }
        }

        Ok(())
    }

    fn wait_receive_one(&mut self) -> RunResult<E> {
        let (id, r) = self.receiver.recv().unwrap();
        let name = self.on_finished(id);
        r.map_err(|e| (name, e))
    }

    pub fn into_wait(mut self) -> RunResult<E> {
        let r = self.wait_receive_all();
        self.handles.clear();
        r
    }

    fn wait_receive_all(&mut self) -> RunResult<E> {
        loop {
            if self.handles.len() == 0 {
                return Ok(());
            }

            self.wait_receive_one()?;
        }
    }

    fn on_finished(&mut self, id: usize) -> String {
        self.handles.remove(&id);
        let name = self.names.remove(&id).expect("on_finished with missing id");
        self.progress.on_finish(&name);
        name
    }
}

impl<E: Send + 'static, P: ProgressListener> Drop for ParRunner<E, P> {
    fn drop(&mut self) {
        let _ = self.wait_receive_all();
    }
}

pub struct NullProgressListener;

impl ProgressListener for NullProgressListener {
    fn on_start(&mut self, _: &str) {}
    fn on_finish(&mut self, _: &str) {}
}

impl<P> ProgressListener for P
where
    P: core::ops::DerefMut,
    P::Target: ProgressListener,
{
    fn on_start(&mut self, name: &str) {
        (**self).on_start(name)
    }

    fn on_finish(&mut self, name: &str) {
        (**self).on_finish(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{
        sync::{Arc, Mutex},
        thread::sleep,
        time::Duration,
    };

    fn run_delayed(
        par_runner: &mut ParRunner<(), NullProgressListener>,
        finished: &Arc<Mutex<Vec<usize>>>,
        delay: u64,
        id: usize,
    ) -> RunResult<()> {
        let clone = Arc::clone(finished);
        par_runner.run(&format!("task-{id}"), move || {
            sleep(Duration::from_millis(delay));
            clone.lock().unwrap().push(id);
            Ok(())
        })
    }

    #[test]
    fn single_task() {
        let mut par_runner = ParRunner::with_parallel(1, NullProgressListener);

        let finished = Default::default();

        run_delayed(&mut par_runner, &finished, 10, 0).unwrap();
        drop(par_runner);

        assert_eq!(*finished.lock().unwrap(), vec![0]);
    }

    #[test]
    fn runs_three_tasks_in_parallel() {
        let mut par_runner = ParRunner::with_parallel(3, NullProgressListener);

        let finished = Arc::new(Mutex::new(Vec::new()));

        let delays = [100, 50, 1];
        for (i, delay) in delays.into_iter().enumerate() {
            run_delayed(&mut par_runner, &finished, delay, i).unwrap();
        }

        drop(par_runner);

        assert_eq!(*finished.lock().unwrap(), vec![2, 1, 0]);
    }

    #[test]
    fn waits_for_single_task_to_finish_before_starting_next() {
        let mut par_runner = ParRunner::with_parallel(1, NullProgressListener);

        let finished = Arc::new(Mutex::new(Vec::new()));

        run_delayed(&mut par_runner, &finished, 10, 0).unwrap();

        run_delayed(&mut par_runner, &finished, 10, 1).unwrap();
        assert_eq!(*finished.lock().unwrap(), vec![0]);
    }

    #[test]
    fn failed_task_returns_err() {
        let mut par_runner = ParRunner::with_parallel(1, NullProgressListener);

        par_runner
            .run("fails", || {
                sleep(Duration::from_millis(10));
                Err(())
            })
            .unwrap();

        assert_eq!(
            par_runner.run("ok", || Ok(())),
            Err((String::from("fails"), ()))
        );
    }

    #[test]
    fn failed_task_returns_err_at_next_opportunity() {
        let mut par_runner = ParRunner::with_parallel(2, NullProgressListener);

        par_runner.run("fails", || Err(())).unwrap();
        sleep(Duration::from_millis(1));

        assert_eq!(
            par_runner.run("ok", || {
                sleep(Duration::from_millis(10));
                Ok(())
            }),
            Err((String::from("fails"), ()))
        );
    }

    #[test]
    fn runs_immediately_if_open_thread() {
        let mut par_runner = ParRunner::with_parallel(2, NullProgressListener);

        let finished = Arc::new(Mutex::new(Vec::new()));

        run_delayed(&mut par_runner, &finished, 9, 0).unwrap();
        run_delayed(&mut par_runner, &finished, 10, 1).unwrap();

        sleep(Duration::from_millis(11));

        run_delayed(&mut par_runner, &finished, 10, 2).unwrap();
        assert_eq!(*finished.lock().unwrap(), vec![0, 1]);
    }

    #[test]
    fn failed_task_into_wait_does_not_wait_for_all_to_finish() {
        let mut par_runner = ParRunner::with_parallel(2, NullProgressListener);

        let finished = Arc::new(Mutex::new(Vec::new()));
        run_delayed(&mut par_runner, &finished, 9, 0).unwrap();

        par_runner.run("fails", || Err(())).unwrap();

        assert_eq!(par_runner.into_wait(), Err((String::from("fails"), ())));
        assert_eq!(*finished.lock().unwrap(), vec![]);
    }
}
