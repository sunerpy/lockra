//! Length-prefixed frames: a 4-byte length, then that many bytes of JSON. Native messaging (the
//! browser and the host process, over stdio) writes the length in the machine's byte order; the
//! local socket between the host and the app writes it little-endian. A frame over [`MAX_FRAME`]
//! is never read: the connection ends.

use std::io;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// The longest frame either way: 1 MiB, what a browser accepts from a native host.
pub const MAX_FRAME: usize = 1 << 20;

/// How a frame's length is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Order {
    /// Little-endian: the local socket.
    Little,
    /// The machine's own: native messaging's stdio.
    Native,
}

impl Order {
    fn decode(self, bytes: [u8; 4]) -> u32 {
        match self {
            Self::Little => u32::from_le_bytes(bytes),
            Self::Native => u32::from_ne_bytes(bytes),
        }
    }

    fn encode(self, length: u32) -> [u8; 4] {
        match self {
            Self::Little => length.to_le_bytes(),
            Self::Native => length.to_ne_bytes(),
        }
    }
}

/// A frame that could not be read or written.
#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    /// Its length is over [`MAX_FRAME`].
    #[error("a frame of {0} bytes is over the limit")]
    TooLong(usize),
    /// The stream ended inside a frame.
    #[error("the stream ended inside a frame")]
    Truncated,
    /// The stream failed.
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// The next frame of `reader`; `None` when the stream ends between frames.
pub async fn read_frame<R: AsyncRead + Unpin>(reader: &mut R, order: Order) -> Result<Option<Vec<u8>>, FrameError> {
    let mut length = [0u8; 4];
    let mut got = 0;
    while got < length.len() {
        match reader.read(&mut length[got..]).await? {
            0 if got == 0 => return Ok(None),
            0 => return Err(FrameError::Truncated),
            n => got += n,
        }
    }
    let length = usize::try_from(order.decode(length)).unwrap_or(usize::MAX);
    if length > MAX_FRAME {
        return Err(FrameError::TooLong(length));
    }
    let mut frame = vec![0u8; length];
    reader.read_exact(&mut frame).await.map_err(|error| if error.kind() == io::ErrorKind::UnexpectedEof { FrameError::Truncated } else { error.into() })?;
    Ok(Some(frame))
}

/// Write `frame` to `writer` and flush it.
pub async fn write_frame<W: AsyncWrite + Unpin>(writer: &mut W, order: Order, frame: &[u8]) -> Result<(), FrameError> {
    if frame.len() > MAX_FRAME {
        return Err(FrameError::TooLong(frame.len()));
    }
    let length = u32::try_from(frame.len()).map_err(|_| FrameError::TooLong(frame.len()))?;
    writer.write_all(&order.encode(length)).await?;
    writer.write_all(frame).await?;
    writer.flush().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn frames_go_through_in_both_byte_orders() {
        for order in [Order::Little, Order::Native] {
            let (mut a, mut b) = tokio::io::duplex(64);
            let sent = b"{\"v\":1}".to_vec();
            let writer = tokio::spawn(async move {
                write_frame(&mut a, order, &sent).await.unwrap();
                write_frame(&mut a, order, b"").await.unwrap();
            });
            assert_eq!(read_frame(&mut b, order).await.unwrap().unwrap(), b"{\"v\":1}");
            assert_eq!(read_frame(&mut b, order).await.unwrap().unwrap(), b"");
            writer.await.unwrap();
            assert!(read_frame(&mut b, order).await.unwrap().is_none(), "the end, between frames");
        }
        let little = Order::Little.encode(258);
        assert_eq!(little, [2, 1, 0, 0]);
        assert_eq!(Order::Native.decode(Order::Native.encode(258)), 258);
    }

    #[tokio::test]
    async fn a_frame_over_the_limit_or_cut_short_ends_the_stream() {
        let too_long = u32::try_from(MAX_FRAME + 1).unwrap().to_le_bytes();
        let mut reader: &[u8] = &too_long;
        assert!(matches!(read_frame(&mut reader, Order::Little).await, Err(FrameError::TooLong(n)) if n == MAX_FRAME + 1));
        let mut reader: &[u8] = &[0xff, 0xff, 0xff, 0xff];
        assert!(matches!(read_frame(&mut reader, Order::Little).await, Err(FrameError::TooLong(_))));
        // The length itself cut short, then the body.
        let mut reader: &[u8] = &[5, 0];
        assert!(matches!(read_frame(&mut reader, Order::Little).await, Err(FrameError::Truncated)));
        let mut reader: &[u8] = &[5, 0, 0, 0, b'{', b'}'];
        assert!(matches!(read_frame(&mut reader, Order::Little).await, Err(FrameError::Truncated)));
        // Exactly the limit is fine; one byte more is refused on writing too.
        let mut sink = Vec::new();
        write_frame(&mut sink, Order::Little, &vec![b' '; MAX_FRAME]).await.unwrap();
        assert_eq!(sink.len(), MAX_FRAME + 4);
        assert!(matches!(write_frame(&mut sink, Order::Little, &vec![b' '; MAX_FRAME + 1]).await, Err(FrameError::TooLong(_))));
    }
}
