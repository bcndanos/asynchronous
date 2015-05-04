use std::thread;
use std::sync::mpsc;

pub struct Promise<T,E> {
	receiver : mpsc::Receiver<Result<T,E>>
}

impl<T,E> Promise<T,E> where T: Send + 'static , E: Send + 'static {

	pub fn new<F>(f:F) -> Promise<T,E> where F: Send + 'static + FnOnce() -> Result<T,E> {
		let (tx,rx) = mpsc::channel();
		thread::spawn(move || { tx.send(f()) });
		Promise::<T,E> { receiver: rx }
	}	

	pub fn all(vector:Vec<Promise<T,E>>) -> Promise<Vec<T>, E> {
		let (tx,rx) = mpsc::channel();
		thread::spawn(move || {
			let mut results = vector.iter().map(|promise| { promise.receiver.recv().unwrap() }); 			
			tx.send(					
				match results.find(|r| r.is_err()) {
					Some(t) => { 					
						let res:Result<Vec<T>,E> = Err(match t { Ok(_) => unreachable!(), Err(e) => e });
						res
					},
					None => {
						let ok_results = results.map(|r| match r { Ok(t) => t, Err(_) => unreachable!() }).collect();
						let res:Result<Vec<T>,E> = Ok(ok_results);
						res
					}
				}
			)
		});
		Promise::<Vec<T>, E> { receiver: rx }
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
		let res = self.receiver.recv().unwrap();
		match res {
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
	use super::*;	

	#[test]
	fn creation() {		
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
}