/*!
A **promise** based asynchronous library

This library provides an usefull way to invoke functions (clousures) in a **Promise Style** using separated threads. A Promise 
is a Struct that represents the return value or the error that the funcion produces. 

It also allows the execution of tasks in Parallel or Series in deferred, joining the result in a Promise.

It includes methods to manage Event Loops, where there are tasks that "emit" events in background, and they are collected by a promise.

[Project github page](https://github.com/bcndanos/asynchronous)

This project is based on the [Q Promise](https://github.com/kriskowal/q) library for Node JS and [Async.js](https://github.com/caolan/async)

#License

Dual-licensed to be compatible with the Rust project.

Licensed under the Apache License, Version 2.0
http://www.apache.org/licenses/LICENSE-2.0 or the MIT license
http://opensource.org/licenses/MIT, at your
option. This file may not be copied, modified, or distributed
except according to those terms.

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
}).finally_sync(|res| {       // res has type Result<u32,&str>
  // Executed always at the end
  assert_eq!(res.unwrap(), 6u32);
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
}).finally_sync(|res| {   // res : Result<Vec<u32>,&str>
    // Do something here    
    assert_eq!(res.unwrap(), vec![4u32, 5u32]);
});

``` 

Simple event loop:

```rust
use asynchronous::EventLoop;

let el = EventLoop::new().finish_in_ms(100);
el.emit("Event1");
el.emit("Event2");
// Do something here
el.to_promise().finally_sync(|res| {  // res: Result<Vec<Ev>,()>
    assert_eq!(res.unwrap(), vec!["Event1", "Event2"]);
});
``` 

*/
#![feature(core)]
extern crate num_cpus;

use std::boxed::FnBox;
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
pub struct Deferred<T,E>  {    
    f   : Box<FnBox() -> Result<T,E> + Send + 'static>,        
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
    pub fn new<F>(f:F) -> Deferred<T,E> 
        where F: Send + 'static + FnOnce() -> Result<T,E> {
        Deferred { f : Box::new(f) }
    } 

    /// Syncronizes the execution with the caller, and returns its value.
    ///
    /// ```rust
    /// use asynchronous::Deferred;
    /// 
    /// let deferred_a = Deferred::<_,&str>::new(|| {
    ///    // Do something  
    ///    Ok("value a")
    /// });
    /// let deferred_b = Deferred::<_,&str>::new(|| {    
    ///    // Do something 
    ///    Ok("value b")
    /// });
    /// // Do something
    /// assert_eq!(deferred_a.sync(), Ok("value a"));
    /// assert_eq!(deferred_b.sync(), Ok("value b"));
    /// ``` 
    pub fn sync(self) -> Result<T,E> {        
        self.to_promise().sync()
    }

    /// Executes only one of the two functions received depending on the result of the previous task deferred (Ok or Err). 
    /// Returns a new Deferred
    ///
    /// ```rust
    /// let r = asynchronous::Deferred::new(|| {
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
    pub fn then<TT,EE,FT,FE>(self,ft:FT,fe:FE) -> Deferred<TT,EE> 
        where   TT: Send + 'static, EE: Send + 'static,
                FT: Send + 'static + FnOnce(T) -> Result<TT,EE>, 
                FE: Send + 'static + FnOnce(E) -> Result<TT,EE> {
        Deferred::<TT,EE> {
            f: Box::new(|| {
                match self.unlock() {
                    Ok(t) => ft(t),
                    Err(e) => fe(e),
                }
            })
        }
    }

    /// The same functionality as **then**, but wraps the result in only one function
    ///
    /// ```rust
    /// let r = asynchronous::Deferred::new(|| {
    ///    if false { Ok(1.23) } else { Err("Final error")}
    /// }).chain(|res| {
    ///    // Do something that executes always even on error
    ///    assert!(res.is_err());
    ///    res
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
    pub fn chain<TT,EE,FF>(self, f:FF) -> Deferred<TT,EE> 
        where   TT: Send + 'static, EE : Send + 'static,         
                FF: Send + 'static + FnOnce(Result<T,E>) -> Result<TT,EE> {
        Deferred::<TT,EE> { f: Box::new(|| { f(self.unlock()) }) }
    }

    /// Executes a new task if the result of the previous promise is Ok. It may return a new type in a correct result (Ok),
    /// but it must return the same type of error of its previous promise.
    ///
    /// ```rust
    /// asynchronous::Deferred::new(|| {
    ///    Ok(1.23)
    /// }).success(|res| {
    ///    assert_eq!(res, 1.23);
    ///    Ok(34)
    /// }).success(|res| {
    ///    assert_eq!(res, 34);
    ///    if true { Ok(res) } else { Err("Final error")}
    /// }).sync();
    /// ```   
    pub fn success<TT,F>(self,f:F) -> Deferred<TT,E> where F: Send + 'static + FnOnce(T) -> Result<TT,E>, TT: Send + 'static {      
        Deferred::<TT,E> {
            f: Box::new(|| {
                match self.unlock() {
                    Ok(t) => f(t),
                    Err(e) => Err(e),
                }
            })
        }
    }

    /// Executes a new task if the result of the previous promise is Err. It may return a new type in a correct result (Ok),
    /// but it must return the same type of error of its previous promise.
    ///
    /// ```rust
    /// asynchronous::Deferred::new(|| {
    ///    Err(32)
    /// }).success(|res| {
    ///    unreachable!();
    ///    Ok(res)
    /// }).fail(|err| {
    ///    assert_eq!(err, 32);
    ///    Ok("Value Ok")
    /// }).sync();
    /// ```       
    pub fn fail<F>(self,f:F) -> Deferred<T,E> where F: Send + 'static + FnOnce(E) -> Result<T,E> {
        Deferred::<T,E> {
            f: Box::new(|| {
                match self.unlock() {
                    Ok(t) => Ok(t),
                    Err(e) => f(e),
                }
            })
        }        
    }

    /// Executes one function with the result of the previous deferred.
    /// It doesn't return anything and it's completly asynchronous.
    ///
    /// ```rust
    /// asynchronous::Deferred::new(|| {
    ///    std::thread::sleep_ms(100);
    ///    if true { Ok(32) } else { Err("Error txt") }
    /// }).finally(|res| { 
    ///    assert_eq!(res.unwrap(), 32);
    /// });
    ///
    /// let a = 2 + 3;  // This line is executed before the above Promise
    /// 
    /// ```   
    pub fn finally<F>(self, f:F) where F: Send + 'static + FnOnce(Result<T,E>) {
        self.to_promise().finally(f);
    }       

    /// Executes one function with the result of the previous deferred.
    /// It doesn't return anything, but it's synchronized with the caller
    ///
    /// ```rust
    /// use asynchronous::Promise;
    /// 
    /// Promise::new(|| {
    ///    std::thread::sleep_ms(100);
    ///    if true { Ok(32) } else { Err("Error txt") }
    /// }).finally_sync(|res| { 
    ///    assert_eq!(res.unwrap(), 32);
    /// });    
    ///
    /// let a = 2 + 3;  // This line is executed after the above Promise
    pub fn finally_sync<F>(self, f:F) where F: Send + 'static + FnOnce(Result<T,E>) {
        self.to_promise().finally_sync(f);
    }           

    /// Executes the task stored and returns a Promise
    ///
    /// ```rust
    /// let deferred = asynchronous::Deferred::new(|| {
    ///    // Do something  
    ///    if true { Ok("value a") } else { Err("Error description") }
    /// });
    /// // deferred is only stored in memory waiting for .sync() or .to_promise()
    /// deferred.to_promise();
    /// // At this point "deferred" is executing
    /// ```             
    pub fn to_promise(self) -> Promise<T,E> {
        Promise::new(self.f)
    }

    /// Executes a vector of tasks and returns a Promise with a vector of values in case that all tasks ended ok.
    /// If there's one or more tasks with error, the promise fails and returns a vector with all the Results.
    ///
    /// ```rust
    /// use asynchronous::{Deferred, ControlFlow};
    /// 
    /// let mut vec_deferred = Vec::new();
    /// for i in 0..5 { vec_deferred.push(Deferred::<_,&str>::new(move || Ok(i) )) }
    /// let promise = Deferred::vec_to_promise(vec_deferred, ControlFlow::ParallelCPUS);
    /// // At this point all tasks in "vec_deferred" are executing
    /// assert_eq!(promise.sync().unwrap(), vec![0,1,2,3,4]);
    /// ```     
    pub fn vec_to_promise(vector:Vec<Deferred<T,E>>, control: ControlFlow) -> Promise<Vec<T>,Vec<Result<T,E>>> {
        match control {
            ControlFlow::Series => Deferred::process_series(vector, 0),
            ControlFlow::Parallel => Deferred::process_parallel(vector, 0, 0, true),
            ControlFlow::ParallelLimit(limit) => Deferred::process_parallel(vector, limit, 0, true),
            ControlFlow::ParallelCPUS => Deferred::process_parallel(vector, num_cpus::get(), 0, true)
        }
    } 

    /// Executes a vector of tasks and returns a Promise with a vector of the first **num_first** tasks that return Ok.
    /// If the **wait** parameter is **true**, the promise waits for all tasks that are running at that moment.
    /// If it's **false**, the promise returns once reaches the **num_first** tasks with Ok.
    /// If it's not possible achieve this number, the promise fails and returns a vector with all the Results.
    ///
    /// ```rust
    /// use asynchronous::{Deferred, ControlFlow};    
    /// 
    /// let mut v = vec![];
    /// for i in 0..20 { v.push(Deferred::<u32, ()>::new(move ||{ Ok(i) })); }
    /// // The first 5 tasks that ended with Ok
    /// let _ = Deferred::first_to_promise(5, false, v, ControlFlow::ParallelLimit(3))   
    /// .finally_sync(|res| {           
    ///    assert_eq!(res.unwrap().len(), 5);                
    /// });
    /// ```     
    pub fn first_to_promise(num_first:usize, wait:bool, vector:Vec<Deferred<T,E>>, control: ControlFlow) -> Promise<Vec<T>,Vec<Result<T,E>>> {
        match control {
            ControlFlow::Series => Deferred::process_series(vector, num_first),
            ControlFlow::Parallel => Deferred::process_parallel(vector, 0, num_first, wait),
            ControlFlow::ParallelLimit(limit) => Deferred::process_parallel(vector, limit, num_first, wait),
            ControlFlow::ParallelCPUS => Deferred::process_parallel(vector, num_cpus::get(), num_first, wait)
        }
    }

    fn process_series(vector:Vec<Deferred<T,E>>, num_first:usize) -> Promise<Vec<T>,Vec<Result<T,E>>> {
        let (tx,rx) = mpsc::channel();
        thread::spawn(move || {
            let mut results:Vec<T> = Vec::new();                            
            for defer in vector {                
                match defer.unlock() {
                    Ok(t) => {
                        results.push(t);
                        if num_first > 0 && results.len() >= num_first { break }
                    },
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

    fn process_parallel(vector:Vec<Deferred<T,E>>, limit:usize, num_first:usize, wait:bool) -> Promise<Vec<T>,Vec<Result<T,E>>> {
        let (tx,rx) = mpsc::channel();
        thread::spawn(move || {
            let mut num_results_received = 0;
            let mut results:Vec<Option<Result<T,E>>> = vec![];
            for _ in 0..vector.len() { results.push(None); }
            let mut it = vector.into_iter();                    
            let (txinter, rxinter) = mpsc::channel();
            let mut id_process = 0;
            let mut active_process = 0;
            let mut is_error = false;
            loop {                
                if active_process > 0 {
                    match rxinter.recv() {
                        Ok(r) => {
                            let finished:(usize,Result<T,E>) = r;
                            if finished.1.is_err() { is_error = true } else { num_results_received += 1 }
                            results[finished.0] = Some(finished.1);                            
                            active_process -= 1;        
                        },
                        Err(_) => break,
                    }                   
                } 
                if !wait && num_first > 0 && num_results_received >= num_first { break }               
                loop {
                    if num_first > 0 && num_results_received >= num_first { break }
                    match it.next() {
                        Some(defer) => {                                                        
                            active_process += 1;                            
                            let txinter_cloned = txinter.clone();
                            thread::spawn(move || {
                                &txinter_cloned.send((id_process, defer.unlock()));
                            });
                            id_process += 1;
                        },
                        None => break
                    }                    
                    if limit > 0 && active_process >= limit { break }
                }
                if active_process == 0 { break }
            }                   
            if num_first > 0 { is_error = num_results_received < num_first }
            let ok_results:Result<Vec<T>, Vec<Result<T,E>>> = match is_error {
                false => {
                    let mut v:Vec<T> = Vec::new();
                    for r in results { 
                        if r.is_none() { continue }
                        v.push(match r.unwrap() { Ok(t) => t, Err(_) => continue })
                    }
                    Ok(v)
                },
                true  => {
                    let mut v:Vec<Result<T,E>> = Vec::new();
                    for r in results { 
                        if r.is_none() { continue }
                        v.push(r.unwrap()) 
                    }
                    Err(v)
                }
            };
            tx.send(ok_results).unwrap()
        });
        Promise::<Vec<T>, Vec<Result<T,E>>> { receiver: rx }
    }    



    fn unlock(self) -> Result<T,E> {
        (self.f)()
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

    /// The same functionality as **then**, but wraps the result in only one function
    ///
    /// ```rust
    /// let r = asynchronous::Promise::new(|| {
    ///    if false { Ok(1.23) } else { Err("Final error")}
    /// }).chain(|res| {
    ///    // Do something that executes always even on error
    ///    assert!(res.is_err());
    ///    res
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
    pub fn chain<TT,EE,F>(self, f:F) -> Promise<TT,EE> 
        where TT: Send + 'static, EE: Send + 'static,
              F: Send + 'static + FnOnce(Result<T,E>) -> Result<TT,EE> {
        let (tx,rx) = mpsc::channel();
        thread::spawn(move || { 
            let res = self.receiver.recv().unwrap();
            tx.send(f(res))
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

    /// Executes one function with the result of the previous promise.
    /// It doesn't return anything and it's completly asynchronous.
    ///
    /// ```rust
    /// asynchronous::Promise::new(|| {
    ///    std::thread::sleep_ms(100);
    ///    if true { Ok(32) } else { Err("Error txt") }
    /// }).finally(|res| { 
    ///    assert_eq!(res.unwrap(), 32);
    /// });
    ///
    /// let a = 2 + 3;  // This line is executed before the above Promise
    /// 
    /// ```   
    pub fn finally<F>(self, f:F) where F: Send + 'static + FnOnce(Result<T,E>) {
        thread::spawn(move || {
            f(self.receiver.recv().unwrap());
        });
    }       

    /// Executes one function with the result of the previous promise.
    /// It doesn't return anything, but it's synchronized with the caller
    ///
    /// ```rust
    /// use asynchronous::Promise;
    /// 
    /// Promise::new(|| {
    ///    std::thread::sleep_ms(100);
    ///    if true { Ok(32) } else { Err("Error txt") }
    /// }).finally_sync(|res| { 
    ///    assert_eq!(res.unwrap(), 32);
    /// });    
    ///
    /// let a = 2 + 3;  // This line is executed after the above Promise
    pub fn finally_sync<F>(self, f:F) where F: Send + 'static + FnOnce(Result<T,E>) {
        f(self.sync());
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
    /// el.finish_in_ms(100).to_promise().finally_sync(|res| {  // res: Result<Vec<Ev>,()>
    ///     assert_eq!(res.unwrap(), vec!["Event Test", "Event Test", "Event Test"]);
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
    pub fn finish(&self) {         
        let mut lock_finished = self.finished.lock().unwrap();
        if !*lock_finished {
           *lock_finished = true;
           let _ = self.tx.send(None);
        }        
    }

    /// Finishes the event loop in N milliseconds
    pub fn finish_in_ms(&self, duration_ms:u32) {
        let handler = self.clone();
        thread::spawn(move|| {
            thread::sleep_ms(duration_ms);    
            let _ = handler.finish();            
        });
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
    /// el.to_promise().finally_sync(|res| {  // res: Result<Vec<Ev>,()>
    ///     assert_eq!(res.unwrap(), vec!["Event1", "Event2"]);
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
    /// el.finish().to_promise().finally_sync(|res| {  // res: Result<Vec<Ev>,()>
    ///     assert_eq!(res.unwrap().len(), 0);
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
    /// el.finish().to_promise().finally_sync(|res| {  // res: Result<Vec<Ev>,()>
    ///     let vec_res = res.unwrap();
    ///     assert_eq!(vec_res.len(), 1);
    ///     match vec_res[0] { MiEvents::Goodbye(vec_res) => assert_eq!(vec_res, 555), _=> () };
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
    /// el.finish_in_ms(100).to_promise().finally_sync(|res| {  // res: Result<Vec<Ev>,()>
    ///     assert_eq!(res.unwrap(), vec!["Event Test", "Event Test", "Event Test"]);
    /// });
    /// ```     
    pub fn emit_until<F>(&self, f:F)  where F : Send + 'static + Fn() -> Emit<Ev> {
        self.handler.emit_until(f);
    }

    /// Finishes the event loop immediately.
    pub fn finish(self) -> EventLoop<Ev> { 
        self.handler.finish();
        self
    }

    /// Finishes the event loop in N milliseconds
    pub fn finish_in_ms(self, duration_ms:u32) -> EventLoop<Ev> {
        self.handler.finish_in_ms(duration_ms);
        self
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
