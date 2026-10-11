# mire-byteio

三个独立的字节读取器。字节读取接口不验证 UTF-8，不转换为文本，
不隐式 trim 或过滤内容。

| 类型 | 输入 | 返回结果 |
| --- | --- | --- |
| `VecReader<R>` | `R: BufRead` | `Vec<u8>` |
| `SliceReader<R>` | `R: BufRead` | 借用当前读取器的 `&[u8]` |
| `BytesReader<R>` | `R: Read` | `bytes::BytesMut` |

三个类型各自实现读取方法；没有输出类型族、统一 reader trait 或解析回调。
输入类型 `R` 保留在具体类型中，不进行类型擦除。`R` 支持 `Send` / `Sync` 时，
相应读取器也支持。

## 使用

```rust
use std::io::{self, BufReader};

use bytes::BytesMut;
use mire_byteio::{BytesReader, SliceReader, DelimPolicy, VecReader};

let input = b"\xff\0\r\nnext\n".as_slice();

let mut reader = VecReader::new(BufReader::with_capacity(4, input));
let owned: Vec<u8> = reader.read_line(DelimPolicy::Exclude)?.unwrap();

let mut reader = SliceReader::new(BufReader::with_capacity(4, input));
let borrowed: &[u8] = reader.read_line(DelimPolicy::Exclude)?.unwrap();
assert_eq!(borrowed, b"\xff\0");

let mut reader = BytesReader::with_capacity(4, input);
let first: BytesMut = reader.read_line(DelimPolicy::Exclude)?.unwrap();
let second: BytesMut = reader.read_line(DelimPolicy::Exclude)?.unwrap();
drop(reader);
assert_eq!(owned, b"\xff\0");
assert_eq!(first.as_ref(), b"\xff\0");
assert_eq!(second.as_ref(), b"next");
# Ok::<(), io::Error>(())
```

`VecReader::new` 和 `SliceReader::new` 直接使用已有的 `BufRead`；文件等原始输入
先包装成标准库 `BufReader`。字节切片本身实现了 `BufRead`，可以直接传入。
`BytesReader::new` 自带缓冲，每次最多填充 8 KiB；`with_capacity(buffer_size, source)`
指定固定填充大小并直接返回读取器。这个大小不是行长度上限，不随长行增长。
配置为零时，`fill_buf` 成功返回空切片，`read_line` 和 `read_delim` 返回 `None`，
即使底层仍有数据；标准 `Read::read` 仍可直接读取源。
没有未交付数据时，较大的标准 `Read` 请求可直接读取到调用者
的目标切片。容量溢出和分配失败由底层容器处理。

## 读取契约

三个类型都有以下方法，返回 `io::Result<Option<对应的字节结果>>`：

| 方法 | 读取边界 |
| --- | --- |
| `read_delim(byte, delim_policy)` | 第一个匹配字节，或 EOF 前的剩余内容 |
| `read_line(delim_policy)` | LF、CRLF，或 EOF 前的剩余内容 |

原始字节通过标准 `Read::read(&mut [u8])` 或 `BufRead::fill_buf` / `consume` 读取。

`DelimPolicy::Include` 保留终止符，`Exclude` 排除终止符；两者都消费完整终止符。
`read_line(Exclude)` 去掉 LF 及其紧邻的一个 CR；孤立 CR 保留。
`read_delim(b'\n', Exclude)` 只去掉 LF，保留前面的 CR。

- 空输入返回 `None`；空行、连续换行产生 `Some(empty)`。末尾无换行的非空内容
  返回一次；末尾终止符之后不额外产生一行。
- 短读不代表 EOF。行与 CRLF 可跨读取边界；没有固定行长度上限。底层对非空目标
  返回零，或 `BufRead` 返回空缓冲时，本次到达 EOF。后续调用仍可尝试读取，
  是否有追加数据由底层源决定。
- I/O 错误保留已成功读取但尚未交付的前缀；重试或换读取方法均可继续使用。
  保留前缀不会回退底层输入源的位置。
- 高层读取在填充阶段重试 `Interrupted`。标准 `fill_buf` 可直接传播底层错误；
  `SliceReader` 最终重新借用底层缓冲时的错误也原样返回，不消费该段。
- 标准 `Read`、`BufRead` 与字节读取方法共享位置。切换方法时先使用尚未消费的字节。
  借用切片仍在使用时，Rust 禁止再次可变读取。
- 丢弃读取器会丢弃它持有的未交付前缀及其拥有的输入缓冲。借用外部 `BufRead` 的
  `SliceReader` 在丢弃时提交已成功交付切片的消费量，避免外部源重复交付该段。
- 分配失败遵循标准分配器行为，不保证能作为可恢复的 I/O 错误返回。

Vec 和 BytesMut 可跨下一次读取、读取器销毁而独立持有和修改。Slice 的切片只在当前借用期间
有效。更多例子见 [examples/reading.rs](examples/reading.rs)。

实现依据与分配分析见 [DESIGN.md](DESIGN.md)；性能测量见 [BENCHMARKS.md](BENCHMARKS.md)。
