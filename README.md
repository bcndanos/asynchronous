# asynchronous

**Asynchronous promises in rust using threads**


|Crate|Travis|
|:------:|:-------:|
|[![](http://meritbadge.herokuapp.com/asynchronous)](https://crates.io/crates/asynchronous)|[![Build Status](https://travis-ci.org/bcndanos/asynchronous.svg?branch=master)](https://travis-ci.org/bcndanos/asynchronous)|

| [API Documentation](http://bcndanos.github.io/asynchronous/asynchronous/) |

#Overview

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
