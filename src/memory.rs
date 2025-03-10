use std::{
    error::Error,
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

#[derive(Debug)]
pub struct Memory<const MAX: usize> {
    array: [u8; MAX],
}

impl<const MAX: usize> Default for Memory<MAX> {
    fn default() -> Self {
        Memory { array: [0; MAX] }
    }
}

impl<const MAX: usize> Memory<MAX> {
    // プログラムをロードする関数
    // 将来的にはロードする位置を指定できるようにしたい。
    // 遅延ロードとかもやってみたい。割と遅延ロードにするといいかもしれない気がする。
    pub fn load<P: AsRef<Path>>(
        &mut self,
        filename: P,
    ) -> core::result::Result<(), Box<dyn Error>> {
        let file = File::open(filename)?;
        let mut reader = BufReader::new(file);

        let mut buf = Vec::new();

        let n = reader.read_to_end(&mut buf)?;

        if n > MAX {
            panic!("Error: The file size is too big or MAX is too small.");
        }

        self.array[..n].copy_from_slice(&buf[..n]);
        self.array[n..].fill(0);

        Ok(())
    }

    // メモリを読み出す関数
    // SIZEでメモリのサイズを指定する。SIZEが最大メモリの大きさを超えた場合はパニックになる。
    // addressで読み込みたいメモリの位置を指定する。addressの大きさが最大メモリの大きさを超えている場合は余りになる。
    // address + SIZEが最大メモリの大きさを超えている場合はRISC-Vの仕様では0番地に戻る。
    pub fn read<const SIZE: usize>(&self, address: usize) -> [u8; SIZE] {
        if SIZE / MAX > 1 {
            panic!("Error: SIZE is too big.");
        }

        let address = address % MAX;

        let mut array = [0; SIZE];

        if address + SIZE > MAX {
            let diff = MAX - address;

            array.copy_from_slice(&self.array[address..]);
            array.copy_from_slice(&self.array[..SIZE - diff]);
        } else {
            array.copy_from_slice(&self.array[address..address + SIZE]);
        }

        array
    }

    // メモリに書き出す関数
    // addressで書き込みたいメモリの位置を指定する。addressの大きさが最大メモリの大きさを超えている場合は余りになる。
    // valuesで書き込みたい配列を指定する。valuesのサイズが最大メモリの大きさを超えた場合はパニックになる。
    // address + valuesのサイズが最大メモリの大きさを超えている場合はRISC-Vの仕様では0番地に戻る。
    pub fn write(&mut self, address: usize, values: &[u8]) {
        let size = values.len();

        if size / MAX > 1 {
            panic!("Error: The size of values is too big.");
        }

        let address = address % MAX;

        if address + size > MAX {
            let diff = MAX - address;

            self.array[address..].copy_from_slice(&values[..diff]);
            self.array[..diff].copy_from_slice(&values[diff..]);
        } else {
            self.array[address..].copy_from_slice(values);
        }
    }

    // メモリの内容を表示する関数
    // デバッグ用なので消す予定。
    pub fn show_array(&self) {
        println!("{:?}", &self.array[..100]);
    }
}
