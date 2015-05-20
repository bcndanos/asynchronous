extern crate asynchronous;
extern crate rustc_serialize;
extern crate docopt;
extern crate ansi_term;

use asynchronous::{Promise, EventLoop, EventLoopHandler, Emit};
use docopt::Docopt;
use std::thread;
use std::io;
use std::io::Write;
use std::sync::{Arc,Mutex};
use ansi_term::Colour::{Green, Yellow, Red};

static USAGE: &'static str = "
Usage:
  cli submit [--duration=<duration>]
  cli stop <id>
  cli status
  cli exit

Options:
  --duration=<duration>       Execution duration in milliseconds
";

static USAGE_INFO: &'static str = "
Usage:
  submit [--duration=<duration>]
  stop <id>
  status
  exit

Options:
  --duration=<duration>       Execution duration in milliseconds  
";

#[derive(RustcDecodable, Debug)]
struct Args {
    cmd_submit: bool,
    cmd_stop: bool,
    cmd_status: bool,   
    cmd_exit: bool, 
    arg_id: Option<usize>,
    flag_duration: Option<u32>,
}

enum Execute {
    Submit(Option<u32>),
    Stop(usize),
    Status,
    Exit
}

enum Job {
    Update(usize, u64)
}

struct Jobs {
    active   : Vec<(usize, EventLoopHandler<Job>, u64)>,
    ended    : Vec<(usize, EventLoopHandler<Job>, u64)>,
}

impl Jobs {
    fn new() -> Jobs{
        Jobs {
            active :vec![],
            ended : vec![],
        }
    }

    fn get_status(&self) -> (usize, usize) {
        (self.active.len(), self.ended.len())
    }

    fn add(&mut self, handler:EventLoopHandler<Job>) -> usize {
        let id = self.active.len() + self.ended.len() + 1;        
        self.active.push((id, handler, 1));
        id
    }

    fn stop(&mut self, id:usize) {
        let position = self.active.iter().enumerate().find(|r| (r.1).0 == id).unwrap().0;                
        self.ended.push(self.active.remove(position));            
    }   

    fn is_active(&self, id:usize) -> bool {        
        self.active.iter().find(|r| r.0 == id).is_some()
    }

    fn find(&self, id:usize) -> Option<&EventLoopHandler<Job>> {
        let rec = self.active.iter().find(|r| r.0 == id);                
        if rec.is_none() { return None }
        Some(&rec.unwrap().1)
    }

    fn set_prime_number(&mut self, id:usize, prime:u64) {
        let rec = self.active.iter_mut().find(|r| r.0 == id);                
        if rec.is_none() { return }
        rec.unwrap().2 = prime;
    }

    fn get_prime_number(&self, id:usize) -> u64 {
        let rec = self.active.iter().find(|r| r.0 == id);                
        if rec.is_none() { return 1 }
        rec.unwrap().2
    }

    fn get_active_jobs(&self) -> Vec<(usize, u64)> {
        self.active.iter().map(|v| (v.0, v.2)).collect()
    }

    fn get_ended_jobs(&self) -> Vec<(usize, u64)> {
        self.ended.iter().map(|v| (v.0, v.2)).collect()
    }    
}

fn main() {

    println!("{}", Green.paint(USAGE_INFO));

    let jobs = Arc::new(Mutex::new(Jobs::new()));

    let el = EventLoop::on_managed(move |event| {
        match event {
            Execute::Submit(duration) => {
                submit_job(jobs.clone(), duration).then(|res| {
                    let st = format!("Job {} submitted successfully!", res);
                    println!("{}", Yellow.paint(&st));
                    Ok(res)
                }, |err| {
                    println!("There was an error submitting a job!");
                    Err(err)
                }).finally_wrap(|_| { 
                    show_command_line();   
                }); 
            },
            Execute::Stop(id) => { 
                stop_job(jobs.clone(), id).fail(move |err| {
                    let st = format!("Error stopping job {} : {}", id, err);
                    println!("{}", Red.paint(&st));
                    show_command_line();
                    Err(err)
                }); 
            },
            Execute::Status => { show_status(jobs.clone()); },
            Execute::Exit => return Emit::Stop,
        };
        Emit::Continue
    });
    
    el.emit_until(|| {
        show_command_line();
        let ref mut command = String::new();
        let docopt: Docopt = Docopt::new(USAGE).unwrap_or_else(|error| error.exit());
        let _ = io::stdin().read_line(command);
        let x: &[_] = &['\r', '\n'];
        let mut raw_args: Vec<&str> = command.trim_right_matches(x).split(' ').collect();
        raw_args.insert(0, "cli");
        let args: Args = match docopt.clone().argv(raw_args.into_iter()).decode() {
            Ok(args) => args,
            Err(error) => {
                match error {
                    docopt::Error::Decode(what) => println!("{}", what),
                    _ => println!("{}", Green.paint(USAGE_INFO)) ,
                };
                return Emit::Continue
            },
        };
        if args.cmd_exit { return Emit::Event(Execute::Exit) }
        if args.cmd_submit { return Emit::Event(Execute::Submit(args.flag_duration)) }
        if args.cmd_stop { return Emit::Event(Execute::Stop(args.arg_id.unwrap())) }
        if args.cmd_status { return Emit::Event(Execute::Status) }
        Emit::Continue
    });

    let _ = el.to_promise().finally_wrap_sync(|_| {
        println!("Goodbye!");
    });
}

fn show_command_line() {
    print!(">>> ");
    let _ = io::stdout().flush();
}

fn show_status(jobs:Arc<Mutex<Jobs>>) -> Promise<(),String> {
    Promise::<(),String>::new(||{
        println!("{}", Yellow.paint("Status -> Collecting info ..... "));
        show_command_line();
        thread::sleep_ms(2000);  // Simulate I/O wait 
        Ok(())
    }).then(move |res| {        
        let lock = jobs.lock().unwrap();
        let (active, ended) = lock.get_status();
        let st = format!("Status -> Active Processes: {} ", active);
        println!("{}", Yellow.paint(&st));
        for v in lock.get_active_jobs() {
            let st = format!("                Id Job: {} , Prime Number Found: {}", v.0, v.1);
            println!("{}", Green.paint(&st));
        }
        let st = format!("              Ended Processes : {} ", ended);
        println!("{}", Yellow.paint(&st));
        for v in lock.get_ended_jobs() {
            let st = format!("                Id Job: {} , Last Prime Number Found: {}", v.0, v.1);
            println!("{}", Red.paint(&st));
        }        
        Ok(res)
    },|err| {
        let st = format!("Status -> Error calculating: {}", err);
        println!("{}", Yellow.paint(&st));
        Err(err)
    }).chain(|res| {
        show_command_line();    
        res
    })   
}

fn submit_job(jobs:Arc<Mutex<Jobs>>, duration:Option<u32>) -> Promise<usize,String> {    

    Promise::new(move || {
        thread::sleep_ms(2000); // Simulate I/O wait         

        let jobs_cloned = jobs.clone();
        let mut event_loop_job = EventLoop::on(move |event| {
            match event {
                Job::Update(id, prime) => {
                    let mut lock = jobs_cloned.lock().unwrap();
                    lock.set_prime_number(id, prime);
                }
            }
        });                                
        if duration.is_some() { event_loop_job = event_loop_job.finish_in_ms(duration.unwrap()); }        

        let mut lock = jobs.lock().unwrap();
        let new_id = lock.add(event_loop_job.get_handler());        

        execute_job(jobs.clone(), new_id, event_loop_job.get_handler());

        let jobs_cloned = jobs.clone();
        event_loop_job.to_promise().finally_wrap(move |_| {
            let mut lock = jobs_cloned.lock().unwrap();
            lock.stop(new_id);
            let st = format!("Job {} ended!", new_id);
            println!("{}", Yellow.paint(&st));
            show_command_line();
        });
        Ok(new_id)
    })
}

fn stop_job(jobs:Arc<Mutex<Jobs>>, id:usize) -> Promise<(),String> {    
    Promise::new(move || {
        {
            let lock = jobs.lock().unwrap();
            if !lock.is_active(id) {
                return Err("Process not active".to_string());
            }
        }
        let st = format!("Stopping Job {} ...", id);
        println!("{}", Yellow.paint(&st));
        show_command_line();        

        thread::sleep_ms(2000); // Simulate I/O wait 
        let jobs_lock = jobs.lock().unwrap();
        match jobs_lock.find(id) {
            Some(job) => Ok(job.finish()),
            None => Err("Error finding job!".to_string())
        }
    })
}

fn execute_job(jobs:Arc<Mutex<Jobs>>, id_job:usize, event_loop_job: EventLoopHandler<Job>) {
    event_loop_job.emit_until(move || {
        thread::sleep_ms(10);
        let lock = jobs.lock().unwrap();
        let next_prime = find_next_prime_number(lock.get_prime_number(id_job));
        Emit::Event(Job::Update(id_job, next_prime))
    });
}

fn is_prime(number:u64) -> bool {
    for i in 2..number {
        if number % i == 0 { return false }
    }
    true
}

fn find_next_prime_number(from:u64) -> u64 {    
    let mut probe = from + 1;
    while !is_prime(probe) { probe += 1; }
    return probe
}