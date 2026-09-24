use std::fs::File;
use std::io;
use std::sync::Arc;

use io::BufRead;
use io::BufReader;

use io::BufWriter;
use io::Write;

use arrow_ipc::writer::StreamWriter;

use arrow_array::Float32Array;
use arrow_array::RecordBatch;

use arrow_schema::DataType;
use arrow_schema::Field;
use arrow_schema::Schema;
use arrow_schema::SchemaRef;

/// Gets the unpacked float values.
///
/// Example:
///
/// ```md
/// originals: 1.0, 2.5, 3.0
/// max-min: 2.0
/// scale: 127.5
/// intercept: 1.0
/// originals - intercept: 0.0, 1.5, 2.0
/// packed: 0, 191, 255
/// unpacked: 0, 1.498, 2
/// decoded: 1.0, 2.498, 3.0
/// ```
pub fn bytes2floats(
    packed_floats8bit32x: &[u8; 32], // 32B
    refval: f32,                     // 4B
    scale_inv: f32,                  // 4B
) -> Float32Array {
    let mut v: Vec<f32> = Vec::with_capacity(32);
    for packed in packed_floats8bit32x {
        let converted: f32 = (*packed).into(); // 0.0-255.0
        let scaled: f32 = converted * scale_inv;
        let offset: f32 = refval + scaled;
        v.push(offset);
    }
    v.into()
}

pub trait BatSink {
    fn sink(&mut self, bat: &RecordBatch) -> Result<(), io::Error>;
    fn close(self) -> Result<(), io::Error>;
}

pub struct StreamWtr<W>(pub StreamWriter<W>)
where
    W: Write;

impl<W> BatSink for StreamWtr<W>
where
    W: Write,
{
    fn sink(&mut self, bat: &RecordBatch) -> Result<(), io::Error> {
        self.0.write(bat).map_err(io::Error::other)
    }

    fn close(mut self) -> Result<(), io::Error> {
        self.0.flush().map_err(io::Error::other)?;
        self.0.finish().map_err(io::Error::other)
    }
}

pub fn wtr2sink<W>(wtr: W, sch: &Schema) -> Result<impl BatSink, io::Error>
where
    W: Write,
{
    let swtr: StreamWriter<_> = StreamWriter::try_new(wtr, sch).map_err(io::Error::other)?;
    Ok(StreamWtr(swtr))
}

pub fn arrays2bat2sink<I, S>(arrays: I, mut sink: S, sref: SchemaRef) -> Result<(), io::Error>
where
    I: Iterator<Item = Result<Float32Array, io::Error>>,
    S: BatSink,
{
    for rarr in arrays {
        let arr: Float32Array = rarr?;
        let rbat: RecordBatch =
            RecordBatch::try_new(sref.clone(), vec![Arc::new(arr)]).map_err(io::Error::other)?;
        sink.sink(&rbat)?;
    }
    sink.close()
}

pub fn colname2schema(colname: &str) -> Schema {
    Schema::new(vec![Field::new(colname, DataType::Float32, false)])
}

pub fn readers2iter_le<P, F>(
    mut packed: P,
    mut foref: F,
) -> impl Iterator<Item = Result<Float32Array, io::Error>>
where
    P: BufRead,
    F: BufRead,
{
    let mut packed_buf: [u8; 32] = [0; 32];

    let mut refval_buf: [u8; 4] = [0; 4];
    let mut scaleinv_buf: [u8; 4] = [0; 4];

    std::iter::from_fn(move || {
        let rp: Result<_, _> = packed.read_exact(&mut packed_buf);
        let rr: Result<_, _> = foref.read_exact(&mut refval_buf);
        let rs: Result<_, _> = foref.read_exact(&mut scaleinv_buf);

        let r: Result<(), io::Error> = (|| {
            rp?;
            rr?;
            rs?;
            Ok(())
        })();

        match r {
            Ok(_) => {
                let pckd: &[u8; 32] = &packed_buf;
                let rval: f32 = f32::from_le_bytes(refval_buf);
                let sval: f32 = f32::from_le_bytes(scaleinv_buf);
                let farr: Float32Array = bytes2floats(pckd, rval, sval);
                Some(Ok(farr))
            }
            Err(e) => match e.kind() {
                io::ErrorKind::UnexpectedEof => None,
                _ => Some(Err(e)),
            },
        }
    })
}

pub fn readers2iter2sink_le<P, F, S>(
    packed: P,
    foref: F,
    sink: S,
    sref: SchemaRef,
) -> Result<(), io::Error>
where
    P: BufRead,
    F: BufRead,
    S: BatSink,
{
    let iarr = readers2iter_le(packed, foref);
    arrays2bat2sink(iarr, sink, sref)
}

#[derive(Debug)]
pub struct BasicConfig {
    pub foref_filename: String,
    pub column_name: String,
}

impl BasicConfig {
    pub fn to_schema(&self) -> SchemaRef {
        let s: Schema = colname2schema(&self.column_name);
        s.into()
    }
}

impl BasicConfig {
    pub fn stdin2packed2fs2ref2iter2sink<S>(
        &self,
        sink: S,
        sref: SchemaRef,
    ) -> Result<(), io::Error>
    where
        S: BatSink,
    {
        let packed = io::stdin().lock();
        let foref_f: File = File::open(&self.foref_filename)?;
        readers2iter2sink_le(packed, BufReader::new(foref_f), sink, sref)
    }
}

impl BasicConfig {
    pub fn stdin2packed2fs2ref2iter2stdout(&self) -> Result<(), io::Error> {
        let sref: SchemaRef = self.to_schema();
        let o = io::stdout();
        let mut ol = o.lock();
        let sink = wtr2sink(BufWriter::new(&mut ol), &sref)?;
        self.stdin2packed2fs2ref2iter2sink(sink, sref.clone())?;
        ol.flush()
    }
}

#[cfg(test)]
mod tests {
    mod bytes2floats {
        use arrow_array::Float32Array;

        #[test]
        fn zeros() {
            let packed: [u8; 32] = [0; 32];
            let refval: f32 = 0.0;
            let scale_inv: f32 = 1.0;
            let arr: Float32Array = crate::bytes2floats(&packed, refval, scale_inv);
            let values: &[f32] = arr.values();
            let expected: [f32; 32] = [0.0; 32];
            assert_eq!(values, &expected);
        }

        #[test]
        fn linear_example() {
            let mut packed: [u8; 32] = [0; 32];
            packed[0] = 100;
            let refval: f32 = 10.0;
            let scale_inv: f32 = 2.0;
            let arr: Float32Array = crate::bytes2floats(&packed, refval, scale_inv);
            let values: &[f32] = arr.values();
            let mut expected: [f32; 32] = [10.0; 32];
            expected[0] = 210.0;
            assert_eq!(values, &expected);
        }

        #[test]
        fn scale_inv_zero() {
            let packed: [u8; 32] = [255; 32];
            let refval: f32 = 42.0;
            let scale_inv: f32 = 0.0;
            let arr: Float32Array = crate::bytes2floats(&packed, refval, scale_inv);
            let values: &[f32] = arr.values();
            let expected: [f32; 32] = [42.0; 32];
            assert_eq!(values, &expected);
        }
    }
}
