/*!
A **promise** based asynchronous library

This library provides an usefull way to invoke functions (clousures) in a **Promise Style** using separated threads. A Promise 
is a Struct that represents the return value or the error that the funcion produces. 

It also allows the execution of tasks in Parallel or Series in deferred, joining the result in a Promise.

It includes methods to manage Event Loops, where there are tasks that "emit" events in background, and they are collected by a promise.

[Project github page](https://github.com/bcndanos/asynchronous)

This project is based on the [Q Promise](https://github.com/kriskowal/q) library for Node JS and [Async.js](https://github.com/caolan/async)

# Examples

This is a simple setup for a promise based execution:

```rust
use asynchronous::Promise;
 
Promise::new(|| {
  // Do something  
  let ret = 10.0 / 3.0;
  if ret > 0.0 { Ok(ret) } else { Err("Incorrect Value") }
}).success(|res| {            // res has type f64
  // Do something if the previous result is correct
  assert_eq!(res, 10.0 / 3.0);
  let res_int = res as u32 * 2;
  Ok(res_int)
}).finally_sync(|res| {       // res has type u32
  // Catch a correct result
  assert_eq!(res, 6u32);
}, |error| {
  // Catch an incorrect result
  unreachable!();
});
``` 

Deferred execution:

```rust
use asynchronous::{Promise,Deferred,ControlFlow};

let d_a = Deferred::<&str, &str>::new(|| { Ok("a") });
let p_b = Promise::<&str, &str>::new(|| { Ok("b") });  // Executed right now
let d1 = Deferred::<u32, &str>::new(|| { Ok(1u32) });
let d2 = Deferred::<u32, &str>::new(|| { Err("Mock Error") });
let d3 = Deferred::<u32, &str>::new(|| { Ok(3u32) });
let d4 = Deferred::<u32, &str>::new(|| { Ok(4u32) });
let d5 = Deferred::<u32, &str>::new(|| { Ok(5u32) });

let promise = Deferred::vec_to_promise(vec![d1,d2,d3], ControlFlow::Parallel);
// Only d1, d2 and d3 are being executed at this time.

assert_eq!("ab", d_a.to_promise().success(|res_a| {
    p_b.success(move |res_b| {
        Ok(res_a.to_string() + res_b)
    }).sync()
}).sync().unwrap());

promise.success(|res| {
    // Catch the result. In this case, tasks d4 and d5 never will be executed
    Ok(res)
}).fail(|error| {
    // Catch the error and execute another Promise
    assert_eq!(error, vec![Ok(1u32), Err("Mock Error"), Ok(3u32)]);    
    Deferred::vec_to_promise(vec![d4,d5], ControlFlow::Series).sync()
}).finally_sync(|res| {   // res : Vec<u32>
    // Do something here    
    assert_eq!(res, vec![4u32, 5u32]);
}, |error| {              // error : Vec<Result<u32,&str>>
    // Check Errors
});

``` 

Simple event loop:

```rust
use asynchronous::EventLoop;

let el = EventLoop::new().finish_in_ms(100);
el.emit("Event1");
el.emit("Event2");
// Do something here
el.to_promise().finally_sync(|res| {  // res: Vec<Ev>
    assert_eq!(res, vec!["Event1", "Event2"]);
}, |error| {
    // Check Errors
});
``` 

*/
extern crate num_cpus;

use std::thread;
use std::sync::{mpsc, Arc, Mutex, Condvar};

/// Different possibilities for asynchronous treatment of vectors
pub enum ControlFlow {
    /// Executes one after each other. Stops execution if one fails.
    Series,  
    /// Executes all tasks at the same time
    Parallel,
    /// Executes all tasks but only **usize** tasks running at the same time
    ParallelLimit(usize),
    /// Executes all tasks but only **NumberCpus** tasks running at the same time
    ParallelCPUS
}

/// Stores a function and delays its execution until it's transform to a promise.
/// T : Type of value returned
/// E : Type of error returned
pub struct Deferred<T,E> {
    starter  : Arc<(Mutex<bool>, Condvar)>,
    receiver : mpsc::Receiver<Result<T,E>>
}

impl<T,E> Deferred<T,E> where T: Send + 'static , E: Send + 'static {
    /// Create a new task in deferred.
    ///
    /// ```rust
    /// let deferred = asynchronous::Deferred::new(|| {
    ///    // Do something  
    ///    if true { Ok("value a") } else { Err("Error description") }
    /// });
    /// // At this point "deferred" is not executed
    /// ```         
    pub fn new<F>(f:F) -> Deferred<T,E> where F: Send + 'static + FnOnce() -> Result<T,E> {
        let (tx,rx) = mpsc::channel();
        let pair = Arc::new((Mutex::new(false), Condvar::new()));
        let pair_c = pair.clone();
        thread::spawn(move|| {
            // wait for the thread to start up
            let &(ref lock, ref cvar) = &*pair_c;
            let mut started = lock.lock().unwrap();    
            while !*started {  started = cvar.wait(started).unwrap(); }            
            tx.send(f())
        });
        Deferred {
            starter  : pair,
            receiver : rx
        }
    }

    /// Chain a deferred to another deferred. 
    ///
    /// ```rust
    /// let deferred = asynchronous::Deferred::new(|| {
    ///    // Do something  
    ///    if true { Ok("value a") } else { Err("Error description") }
    /// }).chain(|res| {
    ///    assert_eq!(res.unwrap(), "value a");
    ///    if true { Ok("value b") } else { Err("Error description") }
    /// });        
    /// ``` 
    pub fn chain<TT,EE,F>(self, f:F) -> Deferred<TT,EE> 
        where   TT: Send + 'static, EE : Send + 'static,         
                F : Send + 'static + FnOnce(Result<T,E>) -> Result<TT,EE> {
        let (tx,rx) = mpsc::channel();
        let pair = Arc::new((Mutex::new(false), Condvar::new()));
        let pair_c = pair.clone();
        thread::spawn(move|| {
            // wait for the thread to start up
            let &(ref lock, ref cvar) = &*pair_c;
            let mut started = lock.lock().unwrap();    
            while !*started {  started = cvar.wait(started).unwrap(); }            
            self.unlock();            
            tx.send(f(self.receiver.recv().unwrap()))
        });
        Deferred::<TT,EE> {
            starter   : pair,
            receiver  : rx
        }
    }

    /// Executes the task stored and returns a Promise
    ///
    /// ```rust
    /// let deferred = asynchronous::Deferred::new(|| {
    ///    // Do something  
    ///    if true { Ok("value a") } else { Err("Error description") }
    /// });
    /// deferred.to_promise();
    /// // At this point "deferred" is executing
    /// ```             
    pub fn to_promise(self) -> Promise<T,E> {
        self.unlock();
        Promise { receiver: self.receiver }
    }

    /// Executes a vector of tasks and returns a Promise with a vector of values in case that all tasks ended ok.
    /// If there's one or more tasks with error, the promise fails and returns a vector with all the Results.
    ///
    /// ```rust
    /// use asynchronous::Deferred;
    /// use asynchronous::ControlFlow;
    /// 
    /// let mut vec_deferred = Vec::new();
    /// for i in 0..5 { vec_deferred.push(Deferred::<_,&str>::new(move || Ok(i) )) }
    /// let promise = Deferred::vec_to_promise(vec_deferred, ControlFlow::ParallelCPUS);
    /// // At this point all tasks in "vec_deferred" are executing
    /// assert_eq!(promise.sync().unwrap(), vec![0,1,2,3,4]);
    /// ```     
    pub fn vec_to_promise(vector:Vec<Deferred<T,E>>, control: ControlFlow) -> Promise<Vec<T>,Vec<Result<T,E>>> {
        match control {
            ControlFlow::Series => Deferred::process_series(vector),
            ControlFlow::Parallel => Deferred::process_parallel(vector, 0),
            ControlFlow::ParallelLimit(limit) => Deferred::process_parallel(vector, limit),
            ControlFlow::ParallelCPUS => Deferred::process_parallel(vector, num_cpus::get())
        }
    } 

    fn process_series(vector:Vec<Deferred<T,E>>) -> Promise<Vec<T>,Vec<Result<T,E>>> {
        let (tx,rx) = mpsc::channel();
        thread::spawn(move || {
            let mut results:Vec<T> = Vec::new();                            
            for defer in vector {
                defer.unlock();
                match defer.receiver.recv().unwrap() {
                    Ok(t) => results.push(t),
                    Err(e) => {
                        let mut results_error:Vec<Result<T,E>> = Vec::new();
                        for t in results { results_error.push(Ok(t)) }
                        results_error.push(Err(e));
                        let res:Result<Vec<T>,Vec<Result<T,E>>> = Err(results_error);
                        tx.send(res).unwrap();
                        return                                
                    }
                }                                                    
            } 
            let ok_results:Result<Vec<T>, Vec<Result<T,E>>> = Ok(results);
            tx.send(ok_results).unwrap()
        });
        Promise::<Vec<T>, Vec<Result<T,E>>> { receiver: rx }           
    }

    fn process_parallel(vector:Vec<Deferred<T,E>>, limit:usize) -> Promise<Vec<T>,Vec<Result<T,E>>> {
        let (tx,rx) = mpsc::channel();
        thread::spawn(move || {
            let mut results:Vec<Option<Result<T,E>>> = vec![];
            for _ in 0..vector.len() { results.push(None); }
            let mut it = vector.into_iter();                    
            let (txinter, rxinter) = mpsc::channel();
            let mut id_process = 0;
            let mut active_process = 0;
            let mut is_error = false;
            loop {
                if active_process > 0 {
                    let finished:(usize,Result<T,E>) = rxinter.recv().unwrap();
                    if finished.1.is_err() { is_error = true }
                    results[finished.0] = Some(finished.1);
                    active_process -= 1;
                }

                loop {
                    match it.next() {
                        Some(defer) => {
                            active_process += 1;
                            defer.unlock();
                            let txinter_cloned = txinter.clone();
                            thread::spawn(move || {
                                let info_send = (id_process, defer.receiver.recv().unwrap());
                                txinter_cloned.send(info_send).unwrap()
                            });
                            id_process += 1;
                        },
                        None => break
                    }
                    if limit!=0 && active_process >= limit { break }
                }
                if active_process == 0 { break }
            }                                      
            let ok_results:Result<Vec<T>, Vec<Result<T,E>>> = match is_error {
                false => {
                    let mut v:Vec<T> = Vec::new();
                    for r in results { v.push(match r.unwrap() { Ok(t) => t, Err(_) => unreachable!() })}
                    Ok(v)
                },
                true  => {
                    let mut v:Vec<Result<T,E>> = Vec::new();
                    for r in results { v.push(r.unwrap()) }
                    Err(v)
                }
            };
            tx.send(ok_results).unwrap()
        });
        Promise::<Vec<T>, Vec<Result<T,E>>> { receiver: rx }
    }

    fn unlock(&self) {
        let &(ref lock, ref cvar) = &*self.starter;        
        let mut started = lock.lock().unwrap();
        *started = true;
        cvar.notify_one();            
    }
}

/// Stores a result of previous execution tasks. 
/// T : Type of value returned
/// E : Type of error returned
pub struct Promise<T,E> {
    receiver : mpsc::Receiver<Result<T,E>>
}

impl<T,E> Promise<T,E> where T: Send + 'static , E: Send + 'static {
    /// Execute a task inmediatly and returns its Promise.
    ///
    /// ```rust
    /// let promise = asynchronous::Promise::new(|| {
    ///    // Do something  
    ///    if true { Ok("value a")} else { Err("Error description") }
    /// });
    /// ```     
    pub fn new<F>(f:F) -> Promise<T,E> where F: Send + 'static + FnOnce() -> Result<T,E> {
        let (tx,rx) = mpsc::channel();
        thread::spawn(move || { tx.send(f()) });
        Promise::<T,E> { receiver: rx }
    }   

    /// Syncronizes the execution with the caller, and returns its value.
    ///
    /// ```rust
    /// use asynchronous::Promise;
    /// 
    /// let promise_a = Promise::<_,&str>::new(|| {
    ///    // Do something  
    ///    Ok("value a")
    /// });
    /// let promise_b = Promise::<_,&str>::new(|| {    
    ///    // Do something 
    ///    Ok("value b")
    /// });
    /// // Do something
    /// assert_eq!(promise_a.sync(), Ok("value a"));
    /// assert_eq!(promise_b.sync(), Ok("value b"));
    /// ``` 
    pub fn sync(self) -> Result<T,E> {
        self.receiver.recv().unwrap()
    }    

    /// Creates a new promise with the result of all other Promises in the vector.
    ///
    /// ```rust
    /// use asynchronous::Promise;
    /// 
    /// let mut vec_promises = Vec::new();
    /// for i in 0..5 { vec_promises.push(Promise::<_,&str>::new(move || Ok(i) )) }
    /// let res = Promise::all(vec_promises).sync().unwrap();
    /// assert_eq!(res, vec![0,1,2,3,4]);
    /// ```     
    pub fn all(vector:Vec<Promise<T,E>>) -> Promise<Vec<T>, Vec<Result<T,E>>> {
        let (tx,rx) = mpsc::channel();
        thread::spawn(move || {           
            let results:Vec<Result<T,E>> = vector.iter().map(|p| p.receiver.recv().unwrap() ).collect();
            let is_error = results.iter().find(|r| r.is_err()).is_some();            
            let ok_results:Result<Vec<T>, Vec<Result<T,E>>> = match is_error {
                false => {
                    let mut v:Vec<T> = Vec::new();
                    for r in results { v.push(match r { Ok(t) => t, Err(_) => unreachable!() })}
                    Ok(v)
                },
                true  => Err(results)
            };
            tx.send(ok_results).unwrap()
        });
        Promise::<Vec<T>, Vec<Result<T,E>>> { receiver: rx }
    }

    /// Executes only one of the two functions received depending on the result of the previous promise (Ok or Err). 
    /// Returns a new Promise
    ///
    /// ```rust
    /// let r = asynchronous::Promise::new(|| {
    ///    if false { Ok(1.23) } else { Err("Final error")}
    /// }).then(|res| {
    ///    unreachable!();
    ///    assert_eq!(res, 1.23);
    ///    Ok(34)
    /// }, |err| {
    ///    assert_eq!(err, "Final error");
    ///    if true { Ok(35) } else { Err(44u64)}
    /// }).sync();
    /// assert_eq!(r, Ok(35));
    /// ```   
    pub fn then<TT,EE,FT,FE>(self,ft:FT,fe:FE) -> Promise<TT,EE> 
        where   TT: Send + 'static, EE: Send + 'static,
                FT: Send + 'static + FnOnce(T) -> Result<TT,EE>, 
                FE: Send + 'static + FnOnce(E) -> Result<TT,EE> {
        let (tx,rx) = mpsc::channel();
        thread::spawn(move || { 
            let res = self.receiver.recv().unwrap();
            match res {
                Ok(t) => tx.send(ft(t)),
                Err(e) => tx.send(fe(e))
            }
        });
        Promise::<TT,EE> { receiver: rx }
    }

    /// Executes a new task if the result of the previous promise is Ok. It may return a new type in a correct result (Ok),
    /// but it must return the same type of error of its previous promise.
    ///
    /// ```rust
    /// asynchronous::Promise::new(|| {
    ///    Ok(1.23)
    /// }).success(|res| {
    ///    assert_eq!(res, 1.23);
    ///    Ok(34)
    /// }).success(|res| {
    ///    assert_eq!(res, 34);
    ///    if true { Ok(res) } else { Err("Final error")}
    /// }).sync();
    /// ```   
    pub fn success<TT,F>(self,f:F) -> Promise<TT,E> where F: Send + 'static + FnOnce(T) -> Result<TT,E>, TT: Send + 'static {      
        let (tx,rx) = mpsc::channel();
        thread::spawn(move || { 
            let res = self.receiver.recv().unwrap();
            match res {
                Ok(t) => tx.send(f(t)),
                Err(e) => tx.send(Err(e))
            }
        });
        Promise::<TT,E> { receiver: rx }
    }

    /// Executes a new task if the result of the previous promise is Err. It may return a new type in a correct result (Ok),
    /// but it must return the same type of error of its previous promise.
    ///
    /// ```rust
    /// asynchronous::Promise::new(|| {
    ///    Err(32)
    /// }).success(|res| {
    ///    unreachable!();
    ///    Ok(res)
    /// }).fail(|err| {
    ///    assert_eq!(err, 32);
    ///    Ok("Value Ok")
    /// }).sync();
    /// ```       
    pub fn fail<F>(self,f:F) -> Promise<T,E> where F: Send + 'static + FnOnce(E) -> Result<T,E> {
        let (tx,rx) = mpsc::channel();
        thread::spawn(move || { 
            let res = self.receiver.recv().unwrap();
            match res {
                Ok(t) => tx.send(Ok(t)),
                Err(e) => tx.send(f(e))
            }
        });
        Promise::<T,E> { receiver: rx }
    }

    /// Executes only one of the two functions received depending on the result of the previous promise (Ok or Err). 
    /// It doesn't return anything and it's completly asynchronous.
    ///
    /// ```rust
    /// asynchronous::Promise::new(|| {
    ///    std::thread::sleep_ms(100);
    ///    if true { Ok(32) } else { Err("Error txt") }
    /// }).finally(|res| { 
    ///    assert_eq!(res, 32);
    /// }, |err|{
    ///    unreachable!();        
    ///    assert_eq!(err, "Error txt");
    /// });
    ///
    /// let a = 2 + 3;  // This line is executed before the above Promise
    /// 
    /// ```           
    pub fn finally<FT,FE>(self, ft:FT, fe:FE) where FT: Send + 'static + FnOnce(T) , FE: Send + 'static + FnOnce(E) {
        thread::spawn(move || {
            let res = self.receiver.recv().unwrap();
            match res {
                Ok(t) => ft(t),
                Err(e) => fe(e)
            };
        });     
    }

    /// Executes only one of the two functions received depending on the result of the previous promise (Ok or Err). 
    /// It doesn't return anything, but it's synchronized with the caller
    ///
    /// ```rust
    /// use asynchronous::Promise;
    /// 
    /// Promise::new(|| {
    ///    std::thread::sleep_ms(100);
    ///    if true { Ok(32) } else { Err("Error txt") }
    /// }).finally_sync(|res| { 
    ///    assert_eq!(res, 32);
    /// }, |err|{
    ///    unreachable!();        
    ///    assert_eq!(err, "Error txt");
    /// });    
    ///
    /// let a = 2 + 3;  // This line is executed after the above Promise
    pub fn finally_sync<FT,FE>(self, ft:FT, fe:FE) where FT: Send + 'static + FnOnce(T) , FE: Send + 'static +FnOnce(E) {
        match self.sync() {
            Ok(t) => ft(t),
            Err(e) => fe(e)
        };
    }   
}

// Event Loops //

/// Enum representing type of Emit in loops
pub enum Emit<Ev> {
    /// Generates a new event
    Event(Ev),   
    /// Continues the loop
    Continue,    
    /// Stops the loop
    Stop         
}

///Event Loop wrapper that can be cloned to pass through threads
pub struct EventLoopHandler<Ev> {
    tx       : mpsc::Sender<Option<Ev>>,    
    finisher : Arc<(Mutex<bool>, Condvar)>,
    finished : Arc<Mutex<bool>>,
}

impl<Ev> Clone for EventLoopHandler<Ev> {
    fn clone(&self) -> EventLoopHandler<Ev> {
        EventLoopHandler {
            tx       : self.tx.clone(),
            finisher : self.finisher.clone(),
            finished : self.finished.clone(),
        }
    }
}

impl<Ev> EventLoopHandler<Ev>  where Ev: Send + 'static {
    /// Triggers an event "Ev" once. Returns the same event if the event loop is not active.
    pub fn emit(&self, event:Ev) -> Result<(), Ev>{         
        if self.is_active() {
            match self.tx.send(Some(event)) {
                Ok(_) => Ok(()),
                Err(e) => Err(e.0.unwrap())            
            }
        } else {
            Err(event)
        }
    }

    /// Triggers an event "Ev" until the return of the "clousure" is None
    /// 
    /// ```rust
    /// use asynchronous::{EventLoop, Emit};
    /// 
    /// let el = EventLoop::new();
    /// let x = std::sync::Arc::new(std::sync::Mutex::new(0));
    /// el.emit_until(move || {
    ///    let mut lock_x = x.lock().unwrap(); *lock_x += 1;
    ///    if *lock_x <= 3 { Emit::Event("Event Test") } else { Emit::Stop }
    /// });    
    /// // Do something here
    /// el.finish_in_ms(100).to_promise().finally_sync(|res| {  // res: Vec<Ev>
    ///     assert_eq!(res, vec!["Event Test", "Event Test", "Event Test"]);
    /// }, |error| {
    ///     // Check Errors
    /// });
    /// ```     
    pub fn emit_until<F>(&self, f:F)  where F : Send + 'static + Fn() -> Emit<Ev> {
        let handler = self.clone();
        thread::spawn(move || {
            loop {                
                match f() {
                    Emit::Event(e) => match handler.emit(e) {
                        Ok(_) => (),
                        Err(_) => break,
                    },
                    Emit::Continue => continue,
                    Emit::Stop => break,
                };
            }
        });
    }    

    /// Returns true if the event loop is running
    pub fn is_active(&self) -> bool {
        let lock_finished = self.finished.lock().unwrap();
        !*lock_finished
    }     

    /// Finishes the event loop immediately.
    pub fn finish(self) -> EventLoopHandler<Ev> { 
        {
            let mut lock_finished = self.finished.lock().unwrap();
            if !*lock_finished {
               *lock_finished = true;
               let _ = self.tx.send(None);
            }
        }
        self
    }

    /// Finishes the event loop in N milliseconds
    pub fn finish_in_ms(self, duration_ms:u32) -> EventLoopHandler<Ev> {
        {
            let handler = self.clone();
            thread::spawn(move|| {
                thread::sleep_ms(duration_ms);    
                let _ = handler.finish();            
            });
        }
        self
    }            

}

///Executes a task in background collecting events from other threads
pub struct EventLoop<Ev> {
    handler  : EventLoopHandler<Ev>,
    receiver : mpsc::Receiver<Ev>,
}

impl<Ev> EventLoop<Ev> where Ev: Send + 'static {

    /// Creates a new Event Loop and collects all the events into a vector
    /// 
    /// ```rust
    /// use asynchronous::EventLoop;
    /// 
    /// let el = EventLoop::new().finish_in_ms(100);
    /// el.emit("Event1");
    /// el.emit("Event2");
    /// // Do something here
    /// el.to_promise().finally_sync(|res| {  // res: Vec<Ev>
    ///     assert_eq!(res, vec!["Event1", "Event2"]);
    /// }, |error| {
    ///     // Check Errors
    /// });
    /// ``` 
    pub fn new() -> EventLoop<Ev> {
        let pair = Arc::new((Mutex::new(false), Condvar::new()));        
        let pair_cloned = pair.clone();
        let (tx,rx) = mpsc::channel();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            loop {
                match rx.recv() { 
                    Ok(t) =>  match t {
                        Some(v) => sender.send(v).unwrap(),
                        None => break 
                    },                    
                    Err(_) => break
                };
            }
            let &(ref lock, ref cvar) = &*pair_cloned;        
            let mut finished = lock.lock().unwrap();
            *finished = true;
            cvar.notify_one();            
        });
        EventLoop {
            handler  : EventLoopHandler {
                tx       : tx,                
                finisher : pair,
                finished : Arc::new(Mutex::new(false)),                
            },
            receiver : receiver,
        }
    }

    /// Creates a new Event Loop and parses all the events produced
    /// 
    /// ```rust
    /// use asynchronous::EventLoop;
    /// enum MiEvents { Hello(String), Goodbye(u32)}
    ///
    /// let el = EventLoop::on(|event| {
    ///    match event {
    ///       MiEvents::Hello(s) => println!("Hello {}", s),
    ///       MiEvents::Goodbye(v) => println!("Goodbye {}", v),
    ///    }
    /// });
    /// el.emit(MiEvents::Hello("World".to_string()));
    /// el.emit(MiEvents::Goodbye(3));
    /// // Do something here
    /// el.finish().to_promise().finally_sync(|res| {  // res: Vec<Ev>
    ///     assert_eq!(res.len(), 0);
    /// }, |error| {
    ///     // Check Errors
    /// });
    /// ```     
    pub fn on<F>(f:F) -> EventLoop<Ev> where F : Send + 'static + Fn(Ev) {      
        let pair = Arc::new((Mutex::new(false), Condvar::new()));    
        let pair_cloned = pair.clone();
        let (tx,rx) = mpsc::channel();
        let (_, receiver) = mpsc::channel();
        thread::spawn(move || {
            loop {
                match rx.recv() { 
                    Ok(t) => match t {
                        Some(v) => f(v) , 
                        None => break
                    },
                    Err(_) => break
                };            
            }
            let &(ref lock, ref cvar) = &*pair_cloned;        
            let mut finished = lock.lock().unwrap();
            *finished = true;
            cvar.notify_one();             
        });
        EventLoop {
            handler  : EventLoopHandler {
                tx       : tx,                
                finisher : pair,
                finished : Arc::new(Mutex::new(false)),                
            },
            receiver : receiver,
        }
    }

    /// Creates a new Event Loop, parses all the events produced and collects what you want into a vector
    /// 
    /// ```rust
    /// use asynchronous::{EventLoop, Emit};
    /// enum MiEvents { Hello(String), Goodbye(u32)}
    /// 
    /// let el = EventLoop::on_managed(|event| {
    ///    match event {
    ///       MiEvents::Hello(s) => { println!("Hello {}", s); Emit::Continue },
    ///       MiEvents::Goodbye(v) => Emit::Event(MiEvents::Goodbye(v)),
    ///    }
    /// });
    /// el.emit(MiEvents::Hello("World".to_string()));
    /// el.emit(MiEvents::Goodbye(555));
    /// // Do something here
    /// el.finish().to_promise().finally_sync(|res| {  // res: Vec<Ev>
    ///     assert_eq!(res.len(), 1);
    ///     match res[0] { MiEvents::Goodbye(v) => assert_eq!(v, 555), _=> () };
    /// }, |error| {
    ///     // Check Errors
    /// });
    /// ```         
    pub fn on_managed<F>(f:F) -> EventLoop<Ev> where F : Send + 'static + Fn(Ev) -> Emit<Ev> {      
        let pair = Arc::new((Mutex::new(false), Condvar::new()));    
        let pair_cloned = pair.clone();
        let (tx,rx) = mpsc::channel();
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            loop {
                let rec = match rx.recv() { 
                    Ok(t) => match t {
                        Some(v) => v , 
                        None => break
                    },
                    Err(_) => break
                };
                match f(rec) {
                    Emit::Event(e) => sender.send(e).unwrap(),
                    Emit::Continue => (),
                    Emit::Stop => break,
                }
            }
            let &(ref lock, ref cvar) = &*pair_cloned;        
            let mut finished = lock.lock().unwrap();
            *finished = true;
            cvar.notify_one();             
        });
        EventLoop {
            handler  : EventLoopHandler {
                tx       : tx,                
                finisher : pair,
                finished : Arc::new(Mutex::new(false)),
            },
            receiver : receiver,
        }        
    }

    /// Triggers an event "Ev" once. Returns the same event if the event loop is not active.
    pub fn emit(&self, event:Ev) -> Result<(), Ev>{         
        self.handler.emit(event)
    }

    /// Triggers an event "Ev" until the return of the "clousure" is None
    /// 
    /// ```rust
    /// use asynchronous::{EventLoop, Emit};
    /// 
    /// let el = EventLoop::new();
    /// let x = std::sync::Arc::new(std::sync::Mutex::new(0));
    /// el.emit_until(move || {
    ///    let mut lock_x = x.lock().unwrap(); *lock_x += 1;
    ///    if *lock_x <= 3 { Emit::Event("Event Test") } else { Emit::Stop }
    /// });    
    /// // Do something here
    /// el.finish_in_ms(100).to_promise().finally_sync(|res| {  // res: Vec<Ev>
    ///     assert_eq!(res, vec!["Event Test", "Event Test", "Event Test"]);
    /// }, |error| {
    ///     // Check Errors
    /// });
    /// ```     
    pub fn emit_until<F>(&self, f:F)  where F : Send + 'static + Fn() -> Emit<Ev> {
        self.handler.emit_until(f);
    }

    /// Finishes the event loop immediately.
    pub fn finish(self) -> EventLoop<Ev> { 
        EventLoop {
            receiver : self.receiver,
            handler  : self.handler.finish(),
        }
    }

    /// Finishes the event loop in N milliseconds
    pub fn finish_in_ms(self, duration_ms:u32) -> EventLoop<Ev> {
        EventLoop {
            receiver : self.receiver,
            handler  : self.handler.finish_in_ms(duration_ms),
        }
    }        

    /// Returns true if the event loop is running
    pub fn is_active(&self) -> bool {
        self.handler.is_active()
    } 

    /// Returns a handler to Event Emitter
    pub fn get_handler(&self) -> EventLoopHandler<Ev> {
        self.handler.clone()
    }

    /// Once the event loop is finished, the promise collects the results.
    pub fn to_promise(self) -> Promise<Vec<Ev>,()> {
        let handler = self.get_handler();
        Promise::new(move || { 
            let &(ref lock, ref cvar) = &*handler.finisher;
            let mut finished = lock.lock().unwrap();    
            while !*finished {  finished = cvar.wait(finished).unwrap(); }                           
            let mut vec = Vec::new();
            loop {
                match self.receiver.recv() {
                    Ok(val) => vec.push(val),
                    Err(_) => break,
                }
            }
            Ok(vec)
        })
    }         
}

#[cfg(test)]
mod test {
    use std::sync::{Arc, Mutex};
    use std::thread;
    use super::*;   

    #[test]
    fn promises() {     
        for x in 0..10 {
            let promise = Promise::<u32,&str>::new(move || { 
                match x {
                    0 => Err("Division by zero"),
                    _ => Ok(x * 2)
                }
            }).success(move |res| {
                assert_eq!(res, x * 2);
                Ok(res * 2) 
            }).fail(|error| {
                assert_eq!(error, "Division by zero");
                Err(error)
            }) ;

            let result = promise.sync();

            match x {
                0 => assert!(result.is_err()),
                _ => {
                    assert!(result.is_ok());
                    assert_eq!(result.unwrap(), x * 4);
                }
            }
        }
    }

    #[test]
    fn promises_parallel() {
        let promise1 = Promise::<u32,&str>::new(|| {
            Ok(1u32)
        });
        let promise2 = Promise::<u32,&str>::new(|| {
            Ok(2u32)
        });
        let promise3 = Promise::<u32,&str>::new(|| {
            Ok(3u32)
        });
        let promise4 = Promise::<u32,&str>::new(|| {
            Err("Error")
        });
        let promise5 = Promise::<u32,&str>::new(|| {
            Ok(5u32)
        });        
        Promise::all(vec![promise1, promise2, promise3]).finally_sync(|res| {
            assert_eq!(res, vec![1u32,2u32,3u32]);
        }, |err| {
            unreachable!("{:?}", err);
        });
        Promise::all(vec![promise4, promise5]).finally_sync(|res| {
            unreachable!("{:?}", res);
        }, |err:Vec<Result<u32,&str>>| {            
            assert!(err[0].is_err());
            assert!(err[1].is_ok());
        });
    }


    #[test]
    fn deferred_to_promise() {        
        Deferred::<u32,&str>::new(|| {                        
            Ok(88u32)
        }).to_promise().finally_sync(|r| {
            assert_eq!(r, 88u32);
        }, |e| {
            panic!("Error not expected {} ", e);
        });        
    }

    #[test]
    fn deferred_in_series() {        
        let st = Arc::new(Mutex::new(String::new()));

        let lock1 = st.clone();
        let d1 = Deferred::<u32, &str>::new(move ||{            
            thread::sleep_ms(200);            
            lock1.lock().unwrap().push_str("Def1");
            Ok(1u32)
        });

        let lock2 = st.clone();
        let d2 = Deferred::<u32, &str>::new(move || {
            thread::sleep_ms(100);
            lock2.lock().unwrap().push_str("Def2");            
            Ok(2u32)
        });

        let lock3 = st.clone();
        let d3 = Deferred::<u32, &str>::new(move ||{
            thread::sleep_ms(200);
            lock3.lock().unwrap().push_str("Def3");            
            Ok(3u32)
        });

        let d4 = Deferred::<u32, &str>::new(|| {
            Ok(4u32)
        }).chain(|res| {
            Ok(res.unwrap() * 3)
        });
        let d5 = Deferred::<u32, &str>::new(|| {
            Err("Error")            
        });
        let d6 = Deferred::<u32, &str>::new(|| {
            Ok(6u32)
        });

        let r = Deferred::vec_to_promise(vec![d1, d2, d3], ControlFlow::Series)
            .success(|res| {
                assert_eq!(vec![1u32,2u32, 3u32], res);
                Ok(0u32)
            }).sync();        
        assert_eq!(r, Ok(0u32));
        assert_eq!(*st.lock().unwrap(),"Def1Def2Def3");

        Deferred::vec_to_promise(vec![d4,d5,d6], ControlFlow::Series)
            .finally_sync(|res| {
                unreachable!("Res: {:?}", res);
            }, |errors| {
                assert_eq!(errors.len(), 2);
                assert_eq!(errors[0], Ok(12u32));
                assert_eq!(errors[1], Err("Error"));
            });
    }

    #[test]
    fn deferred_in_parallel() {
        let st = Arc::new(Mutex::new(String::new()));

        let lock1 = st.clone();
        let d1 = Deferred::<u32, &str>::new(move ||{
            thread::sleep_ms(200);
            lock1.lock().unwrap().push_str("Def1");
            Ok(1u32)
        });
        let lock2 = st.clone();
        let d2 = Deferred::<u32, &str>::new(move || {
            thread::sleep_ms(300);
            lock2.lock().unwrap().push_str("Def2");
            Ok(2u32)
        });
        let lock3 = st.clone();
        let d3 = Deferred::<u32, &str>::new(move ||{
            thread::sleep_ms(50);
            lock3.lock().unwrap().push_str("Def3");
            Ok(3u32)
        });
        let d4 = Deferred::<u32, &str>::new(|| {
            Ok(4u32)
        });
        let d5 = Deferred::<u32, &str>::new(|| {
            Err("Error")            
        });
        let d6 = Deferred::<u32, &str>::new(|| {
            Ok(6u32)
        });

        let r = Deferred::vec_to_promise(vec![d1, d2, d3], ControlFlow::Parallel)
            .success(|res| {
                assert_eq!(vec![1u32,2u32, 3u32], res);
                Ok(0u32)
            }).sync();        
        assert_eq!(r, Ok(0u32));
        assert_eq!(*st.lock().unwrap(),"Def3Def1Def2");
        
        Deferred::vec_to_promise(vec![d4,d5,d6], ControlFlow::Parallel)
            .finally_sync(|res| {
                unreachable!("Res: {:?}", res);
            }, |errors| {
                assert_eq!(errors.len(), 3);
                assert_eq!(errors[0], Ok(4u32));
                assert_eq!(errors[1], Err("Error"));
                assert_eq!(errors[2], Ok(6u32));
            });
    }    

    #[test]
    fn deferred_in_parallel_limit() {
        let st = Arc::new(Mutex::new(String::new()));

        let lock1 = st.clone();
        let d1 = Deferred::<u32, &str>::new(move ||{
            thread::sleep_ms(150);
            lock1.lock().unwrap().push_str("Def1");
            Ok(1u32)
        });
        let lock2 = st.clone();
        let d2 = Deferred::<u32, &str>::new(move || {
            thread::sleep_ms(300);
            lock2.lock().unwrap().push_str("Def2");
            Ok(2u32)
        });
        let lock3 = st.clone();
        let d3 = Deferred::<u32, &str>::new(move ||{
            thread::sleep_ms(50);
            lock3.lock().unwrap().push_str("Def3");
            Ok(3u32)
        });
        let lock4 = st.clone();
        let d4 = Deferred::<u32, &str>::new(move || {
            thread::sleep_ms(200);
            lock4.lock().unwrap().push_str("Def4");
            Ok(4u32)
        });
        
        let d5 = Deferred::<u32, &str>::new(|| {    
            Ok(5u32)
        });
        let d6 = Deferred::<u32, &str>::new(|| {    
            Err("Error d")
        });

        let r = Deferred::vec_to_promise(vec![d1, d2, d3, d4], ControlFlow::ParallelLimit(2))
            .success(|res| {
                assert_eq!(vec![1u32,2u32, 3u32,4u32], res);
                Ok(0u32)
            }).sync();        
        assert_eq!(r, Ok(0u32));
        assert_eq!(*st.lock().unwrap(),"Def1Def3Def2Def4");
        
        Deferred::vec_to_promise(vec![d5,d6], ControlFlow::ParallelLimit(1))
            .finally_sync(|res| {
                unreachable!("Res: {:?}", res);
            }, |errors| {
                assert_eq!(errors.len(), 2);
                assert_eq!(errors[0], Ok(5u32));
                assert_eq!(errors[1], Err("Error d"));
            });
    }        

    #[test]
    fn deferred_in_parallel_limit_cpus() {    
        let mut vec = Vec::new();
        for i in 1..5 {
            vec.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));         
        }
        Deferred::vec_to_promise(vec, ControlFlow::ParallelCPUS)
            .finally_sync(|res| {
                assert_eq!(res, vec![1u32, 2u32, 3u32, 4u32]);
            }, |err| {
                unreachable!("{:?}", err);
            });
    }

    #[test]
    fn deferred_chained() {
        let res = Deferred::<String, &str>::new(||{
            thread::sleep_ms(50);
            if true { Ok("first".to_string()) } else { Err("Nothing") }
        }).chain(|res| {  
            let mut v = res.unwrap();      
            assert_eq!(v, "first");
            thread::sleep_ms(50);
            if true { 
                v.push_str("second"); 
                Ok(v)
            } else { 
                Err("Nothing") 
            }
        }).to_promise().sync().unwrap();
        assert_eq!(res, "firstsecond");
    }

    #[test]
    fn nested_promises() {
        let res = Promise::<_,&str>::new(|| {            
            // Do nothing
            Promise::new(|| {
                Promise::new(|| {
                    Ok(4)
                }).success(|res| {
                    Ok(res + 2)
                }).sync()
            }).success(|res| {
                Ok(res * 7)
            }).sync()
        }).success(|res| {
            Ok(res + 5)
        }).sync().unwrap();
        assert_eq!(res, 47);
    }

    #[test]
    fn event_loop_1() {
        let event_loop = EventLoop::new();
        assert_eq!(event_loop.emit("EventA"), Ok(()));
        assert_eq!(event_loop.emit("EventB"), Ok(()));
        assert_eq!(event_loop.emit("EventC"), Ok(()));
        let res = event_loop.finish().to_promise().sync().unwrap();
        assert_eq!(res, vec!["EventA", "EventB", "EventC"]);
    }

    #[test]
    fn event_loop_2() {
        let event_loop = EventLoop::new().finish_in_ms(50);
        assert_eq!(event_loop.emit("EventA"), Ok(()));
        assert_eq!(event_loop.emit("EventB"), Ok(()));
        thread::sleep_ms(100);
        assert_eq!(event_loop.emit("EventC"), Err("EventC"));
        let res = event_loop.finish().to_promise().sync().unwrap();
        assert_eq!(res, vec!["EventA", "EventB"]);
    }    

    #[test]
    fn event_loop_3() {
        let event_loop = EventLoop::new();
        assert_eq!(event_loop.emit("EventA"), Ok(()));
        assert_eq!(event_loop.emit("EventB"), Ok(()));
        let event_loop = event_loop.finish();
        assert_eq!(event_loop.emit("EventC"), Err("EventC"));
        assert_eq!(event_loop.is_active(), false);
        let res = event_loop.to_promise().sync().unwrap();
        assert_eq!(res, vec!["EventA", "EventB"]);
    }        

    #[test]
    fn event_loop_4() {
        let event_loop = EventLoop::new().finish_in_ms(120);
        assert_eq!(event_loop.emit("EventA"), Ok(()));        
        assert_eq!(event_loop.emit("EventB"), Ok(()));
        let x = Arc::new(Mutex::new(0));
        event_loop.emit_until(move || {
            thread::sleep_ms(25);
            let mut lock_x = x.lock().unwrap(); *lock_x += 1;
            if *lock_x == 2 { return Emit::Continue }
            if *lock_x <= 5 { Emit::Event("EventC") } else { Emit::Stop }
        });
        let res = event_loop.to_promise().sync().unwrap();
        assert_eq!(res, vec!["EventA", "EventB", "EventC", "EventC", "EventC"]);
    }     

    #[test]
    fn event_loop_on_1() {
        let v = Arc::new(Mutex::new(Vec::<&str>::new()));
        let v_cloned = v.clone();
        let event_loop = EventLoop::on(move |event| {
            match event {
                "EventA" => { let mut v_lock = v_cloned.lock().unwrap(); v_lock.push("EventATreated"); },
                "EventB" => (),
                _ => { let mut v_lock = v_cloned.lock().unwrap(); v_lock.push("EventOtherTreated"); },
            }
        });
        assert_eq!(event_loop.emit("EventA"), Ok(()));
        assert_eq!(event_loop.emit("EventB"), Ok(()));
        assert_eq!(event_loop.emit("EventC"), Ok(()));
        let res = event_loop.finish().to_promise().sync().unwrap();
        assert_eq!(res.len(), 0);
    }

    #[test]
    fn event_loop_on_2() {
        enum Event {
            Hello(String),
            Goodbye(String)
        }
        let v = Arc::new(Mutex::new(Vec::<String>::new()));
        let v_cloned = v.clone();
        let event_loop = EventLoop::on(move |event| {
            match event {
                Event::Hello(_) => (),
                Event::Goodbye(v) => { let mut v_lock = v_cloned.lock().unwrap(); v_lock.push(v); },
            }
        }).finish_in_ms(100);
        assert!(event_loop.emit(Event::Hello("World".to_string())).is_ok());
        assert!(event_loop.emit(Event::Goodbye("BCN".to_string())).is_ok());
        assert!(event_loop.emit(Event::Goodbye("MAD".to_string())).is_ok());
        let res = event_loop.to_promise().sync().unwrap();
        assert_eq!(res.len(), 0);
    }    

    #[test]
    fn event_loop_on_managed_1() {
        let event_loop = EventLoop::on_managed(|event| {
            match event {
                "EventA" => Emit::Event("EventATreated"),
                "EventB" => Emit::Continue,
                "EventStop" => Emit::Stop,
                _ => Emit::Event("EventOtherTreated")
            }
        });
        assert_eq!(event_loop.emit("EventA"), Ok(()));
        assert_eq!(event_loop.emit("EventB"), Ok(()));
        assert_eq!(event_loop.emit("EventC"), Ok(()));
        assert_eq!(event_loop.emit("EventStop"), Ok(()));
        thread::sleep_ms(75);
        assert_eq!(event_loop.emit("EventE"), Err("EventE"));
        let res = event_loop.finish().to_promise().sync().unwrap();
        assert_eq!(res, vec!["EventATreated", "EventOtherTreated"]);
    }

    #[test]
    fn event_loop_on_managed_2() {
        enum Event {
            Hello(String),
            Goodbye(String)
        }
        let event_loop = EventLoop::on_managed(|event| {
            match event {
                Event::Hello(_) => Emit::Continue,
                Event::Goodbye(v) => Emit::Event(Event::Goodbye(v)),
            }
        }).finish_in_ms(100);
        assert!(event_loop.emit(Event::Hello("World".to_string())).is_ok());
        assert!(event_loop.emit(Event::Goodbye("BCN".to_string())).is_ok());
        assert!(event_loop.emit(Event::Goodbye("MAD".to_string())).is_ok());
        let res = event_loop.to_promise().sync().unwrap();
        assert_eq!(res.len(), 2);
        match res[0] { Event::Goodbye(ref v) => assert_eq!(v, "BCN") , _ => panic!() };
        match res[1] { Event::Goodbye(ref v) => assert_eq!(v, "MAD") , _ => panic!() };
    }        
}