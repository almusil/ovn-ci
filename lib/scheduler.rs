use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::runner::{Finished, New, Runner, Running};
use crate::Configuration;

#[derive(Debug)]
pub struct Scheduler {
    cpu_itensive: Queue,
    regular: Queue,
}

impl Scheduler {
    pub fn new(config: &Configuration, log_path: &Path) -> Self {
        let mut regular_limit = config.concurrent_limit().unwrap_or(1);
        let cpu_intensive_limit = if regular_limit > 1 {
            (regular_limit / 4) + 1
        } else {
            0
        };

        regular_limit -= cpu_intensive_limit;

        let mut regular = Vec::new();
        let mut cpu_intensive = Vec::new();

        for (i, suite) in config.suites().iter().enumerate() {
            let runner = Runner::new(i, config.vm().memory(), config.jobs(), suite, log_path);

            if cpu_intensive_limit > 0 && suite.is_cpu_intensive() {
                cpu_intensive.push(runner);
            } else {
                regular.push(runner);
            }
        }

        Scheduler {
            cpu_itensive: Queue::new(cpu_intensive, cpu_intensive_limit),
            regular: Queue::new(regular, regular_limit),
        }
    }

    pub fn run(&mut self) {
        while !(self.cpu_itensive.is_finished() && self.regular.is_finished()) {
            self.cpu_itensive.step();
            self.regular.step();

            if self.cpu_itensive.can_yield() {
                self.regular.limit += self.cpu_itensive.limit;
                self.cpu_itensive.limit = 0;
            }

            thread::sleep(Duration::from_millis(100));
        }
    }

    pub fn finished(&self) -> impl Iterator<Item = &Runner<Finished>> {
        self.regular.finished().chain(self.cpu_itensive.finished())
    }
}

#[derive(Debug)]
struct Queue {
    limit: usize,
    waiting: Vec<Runner<New>>,
    running: Vec<Runner<Running>>,
    finished: Vec<Runner<Finished>>,
}

impl Queue {
    fn new(runners: Vec<Runner<New>>, limit: usize) -> Self {
        let runners_len = runners.len();

        Queue {
            limit,
            waiting: runners,
            running: Vec::with_capacity(limit),
            finished: Vec::with_capacity(runners_len),
        }
    }

    fn step(&mut self) {
        self.schedule();
        self.collect_finished();
    }

    fn is_finished(&self) -> bool {
        self.waiting.is_empty() && self.running.is_empty()
    }

    fn can_yield(&self) -> bool {
        self.is_finished() && self.limit > 0
    }

    fn finished(&self) -> impl Iterator<Item = &Runner<Finished>> {
        self.finished.iter()
    }

    fn schedule(&mut self) {
        while !self.waiting.is_empty() && self.running.len() < self.limit {
            if let Some(runner) = self.waiting.pop() {
                println!("{}", runner.report_console());

                match runner.run() {
                    Ok(runner) => self.running.push(runner),
                    Err(runner) => {
                        println!("{}", runner.report_console());
                        self.finished.push(runner);
                    }
                }
            }
        }
    }

    fn collect_finished(&mut self) {
        let indexes = self
            .running
            .iter_mut()
            .enumerate()
            .flat_map(|(i, runner)| runner.try_ready().then_some(i))
            .rev()
            .collect::<Vec<_>>();

        for index in indexes {
            let runner = self.running.swap_remove(index).finish();

            println!("{}", runner.report_console());
            self.finished.push(runner);
        }
    }
}
