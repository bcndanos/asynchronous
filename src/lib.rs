use std::thread;
use std::sync::{mpsc, Arc, Mutex, Condvar};

pub enum ControlFlow {
    Series,
    Parallel,
    //ParallelLimit(u16)
}

pub struct Deferred<T,E> {
    starter  : Arc<(Mutex<bool>, Condvar)>,
    receiver : mpsc::Receiver<Result<T,E>>
}

impl<T,E> Deferred<T,E> where T: Send + 'static , E: Send + 'static {
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

    pub fn to_promise(self) -> Promise<T,E> {
        self.unlock();
        Promise { receiver: self.receiver }
    }

    pub fn vec_to_promise(vector:Vec<Deferred<T,E>>, control: ControlFlow) -> Promise<Vec<T>,Vec<Result<T,E>>> {
        match control {
            ControlFlow::Series => {
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
            },
            ControlFlow::Parallel => {
                let mut vector_promises:Vec<Promise<T,E>> = Vec::new();
                for defer in vector { vector_promises.push(defer.to_promise())};                    
                Promise::all(vector_promises)
            }            
        }
    } 

    fn unlock(&self) {
        let &(ref lock, ref cvar) = &*self.starter;        
        let mut started = lock.lock().unwrap();
        *started = true;
        cvar.notify_one();            
    }
}

pub struct Promise<T,E> {
    receiver : mpsc::Receiver<Result<T,E>>
}

impl<T,E> Promise<T,E> where T: Send + 'static , E: Send + 'static {

    pub fn new<F>(f:F) -> Promise<T,E> where F: Send + 'static + FnOnce() -> Result<T,E> {
        let (tx,rx) = mpsc::channel();
        thread::spawn(move || { tx.send(f()) });
        Promise::<T,E> { receiver: rx }
    }   

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

    pub fn then<TT,F>(self,f:F) -> Promise<TT,E> where F: Send + 'static + FnOnce(T) -> Result<TT,E>, TT: Send + 'static {      
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

    pub fn finally<FT,FE>(self, ft:FT, fe:FE) where FT: Send + 'static + FnOnce(T) , FE: Send + 'static + FnOnce(E) {
        thread::spawn(move || {
            let res = self.receiver.recv().unwrap();
            match res {
                Ok(t) => ft(t),
                Err(e) => fe(e)
            };
        });     
    }

    pub fn finally_sync<FT,FE>(self, ft:FT, fe:FE) where FT: Send + 'static + FnOnce(T) , FE: Send + 'static +FnOnce(E) {
        match self.sync() {
            Ok(t) => ft(t),
            Err(e) => fe(e)
        };
    }   


    pub fn sync(self) -> Result<T,E> {
        self.receiver.recv().unwrap()
    }
}

#[cfg(test)]
mod test {
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
            }).then(move |res| {
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
        // TODO: Acumulate string in Mutex to check serial
        let d1 = Deferred::<u32, &str>::new(||{
            thread::sleep_ms(200);
            Ok(1u32)
        });
        let d2 = Deferred::<u32, &str>::new(|| {
            thread::sleep_ms(100);
            Ok(2u32)
        });
        let d3 = Deferred::<u32, &str>::new(||{
            thread::sleep_ms(200);
            Ok(3u32)
        });
        let d4 = Deferred::<u32, &str>::new(|| {
            thread::sleep_ms(100);
            Ok(4u32)
        });
        let d5 = Deferred::<u32, &str>::new(|| {
            thread::sleep_ms(100);
            Err("Error")            
        });
        let d6 = Deferred::<u32, &str>::new(|| {
            thread::sleep_ms(100);
            Ok(6u32)
        });

        let r = Deferred::vec_to_promise(vec![d1, d2, d3], ControlFlow::Series)
            .then(|res| {
                assert_eq!(vec![1u32,2u32, 3u32], res);
                Ok(0u32)
            }).sync();        
        assert_eq!(r, Ok(0u32));

        Deferred::vec_to_promise(vec![d4,d5,d6], ControlFlow::Series)
            .finally_sync(|res| {
                unreachable!("Res: {:?}", res);
            }, |errors| {
                assert_eq!(errors.len(), 2);
                assert_eq!(errors[0], Ok(4u32));
                assert_eq!(errors[1], Err("Error"));
            });
    }

    #[test]
    fn deferred_in_parallel() {
        // TODO: Acumulate string in Mutex to check parallel
        let d1 = Deferred::<u32, &str>::new(||{
            thread::sleep_ms(200);
            Ok(1u32)
        });
        let d2 = Deferred::<u32, &str>::new(|| {
            thread::sleep_ms(100);
            Ok(2u32)
        });
        let d3 = Deferred::<u32, &str>::new(||{
            thread::sleep_ms(200);
            Ok(3u32)
        });
        let d4 = Deferred::<u32, &str>::new(|| {
            thread::sleep_ms(100);
            Ok(4u32)
        });
        let d5 = Deferred::<u32, &str>::new(|| {
            thread::sleep_ms(100);
            Err("Error")            
        });
        let d6 = Deferred::<u32, &str>::new(|| {
            thread::sleep_ms(100);
            Ok(6u32)
        });

        let r = Deferred::vec_to_promise(vec![d1, d2, d3], ControlFlow::Parallel)
            .then(|res| {
                assert_eq!(vec![1u32,2u32, 3u32], res);
                Ok(0u32)
            }).sync();        
        assert_eq!(r, Ok(0u32));
        
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
}