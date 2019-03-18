use tokio::io;
use tokio::prelude::*;

// https://jsdw.me/posts/rust-futures-tokio/

pub struct ByteStream<R>(pub R);

impl <R: AsyncRead> Stream for ByteStream<R> {
    type Item = Vec<u8>;
    type Error = io::Error;

    // poll is very similar to our Future implementation, except that
    // it returns an `Option<u8>` instead of a `u8`. This is so that the
    // Stream can signal that it's finished by returning `None`:
    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        let mut buf = [0;1024];
        match self.0.poll_read(&mut buf) {
            Ok(Async::Ready(n)) => {
                // By convention, if an AsyncRead says that it read 0 bytes,
                // we should assume that it has got to the end, so we signal that
                // the Stream is done in this case by returning None:
                if n == 0 {
                    Ok(Async::Ready(None))
                } else {
                    Ok(Async::Ready(Some(Vec::from(&buf[..n]))))
                }
            },
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(e) => Err(e)
        }
    }
}
