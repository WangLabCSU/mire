# 设计与实现依据

## 三种独立类型

`VecReader<R: BufRead>`、`SliceReader<R: BufRead>`、`BytesReader<R: Read>`
分别负责 Vec、切片和 BytesMut 交付。三个类型各自实现读取；共用 DelimPolicy 的私有
delim_end 方法计算已终止片段的内容结束位置。没有存储策略 trait、输出 GAT、
交付函数指针或 boxed 动态输入。
`R` 可直接是具体输入，也可以由调用者选择为 `Box<dyn Read>` 等适配形式。

`read_delim` 和 `read_line` 按字节确定读取边界，返回具体字节类型。
标准 `Read` / `BufRead` 用于与标准 I/O 组合，和直接返回字节的方法使用相同位置。
文档中的错误保留契约针对这些字节方法；调用标准 `BufRead::read_until` 时，
已经交付给调用者 Vec 的前缀按标准库契约由调用者持有。

## VecReader

输入缓冲由底层 BufRead 提供。完整段在当前缓冲中时，用 memchr 找到边界，
将应交付内容直接复制到返回的 Vec，消费完整输入段。不建立第二个常驻输入缓冲。

跨缓冲时，把各片段追加到私有 prefix Vec，并消费底层对应字节。完成后可以把
这个 Vec 的分配直接移交给调用者；Vec 已经交付，读取器不能同时保留它用于复用。
追加可能分配或扩容，并搬移已有内容。完成后的移交不再复制整行。

I/O 失败保留 prefix。改变分隔符后，prefix 内可能已经有完整的多个段；每次按需
复制部分内容交付，用游标保留后缀。只在剩余不完整后缀需要继续追加时压紧，避免
每交付一个小段就搬移全部后缀。标准 Read/BufRead 先使用 prefix。

## SliceReader

当前输入缓冲已有完整段时，先确定边界，再借用同一个缓冲，返回其中的切片。
这条路径不分配辅助缓冲、不复制载荷。切片借用当前 `&mut self`，调用者仍在使用
切片时不能继续可变读取，也不能销毁读取器。

返回底层切片时不能同时消费该底层缓冲，因此记录 pending 消费量；下一次可变
操作先提交它。Drop 也提交已经成功交付的 pending，使借用的外部输入源不会再次
交付同一段。最终重新借用失败时不设置 pending，失败输入仍未交付。

跨缓冲时，当前片段必须保存后才能让底层填充，因此复制到 scratch Vec。
完整段随后借用 scratch。后续读取清空已经交付内容，但保留分配容量复用；改变
分隔符或原始读取可推进 scratch 游标，只有追加前才按需压紧后缀。
没有终止符的最后一段也需要先保存内容，再读取确认 EOF，可能使用 scratch。

准备边界与最终借用分开，让实现使用安全 Rust。填充阶段重试 Interrupted。
标准 BufRead 实现在非空缓冲上重新调用 fill_buf 不会再次读取源；如果自定义源
仍在最终借用时返回错误（包括 Interrupted），这里直接传播且不消费输入。
如果该源无故缩短已检查的缓冲，则返回 InvalidData，不进行越界切片。

## BytesReader

构造时直接分配 BytesMut，不将容量溢出转换为 I/O 错误。源字节直接读进 BytesMut。
完整段在当前输入缓冲内时，memchr 定位边界后，用 split_to 分离已消费的前缀，
按终止符模式 truncate，直接返回 BytesMut。未交付后缀继续留在读取器中。
结果和后缀占据互不重叠的区域，可以分别修改。
这减少的是“缓冲中已有字节复制到交付结果”的一步；没有先生成 Vec 再转 BytesMut。

输入缓冲为空时才填充，每次请求固定的 buffer_size 个字节。
零配置保留为零；填充不提供可写空间，成功时返回空切片，因此按行或按分隔符读取
直接返回 None，即使底层仍有数据。标准 Read::read 可绕过缓冲直接读取源。
短读只交付实际读取的内容。长行不会改变后续填充大小；跨块内容累积到局部 out，
其增长由 BytesMut 处理。out 使用 Option<BytesMut>，初始为 None；完整段直接从
输入缓冲切分交付，跨块时才保存 Some(prefix)。跨块路径通过 BytesMut::clone
复制第一块的有效字节，消费对应输入后保留输入分配，供后续填充复用；后续块通过
extend_from_slice 复制追加，必要时还可能搬移已累积内容。
跨块段在当前缓冲中结束时，直接追加至分隔符，再消费该前缀，保留未交付后缀；
这条路径不再先切分终止块。
取得完整段后统一应用分隔符策略，因此 CR 和 LF 分属两块也能正确处理。

已累积前缀时，I/O 错误返回前把 out 移回输入缓冲，不复制前缀字节。
填充只发生在输入缓冲为空时，
失败区域也已清空，因此这个移交不会覆盖未读后缀；原空缓冲的分配可能随替换释放。
后续所有读取方法都先使用输入缓冲；更换分隔符后重新搜索其中的保留前缀，split_to
交付完整段并留下后缀。同一次调用只搜索各个新块，不重复搜索逐渐增长的局部 out。

安全 Read 要求初始化目标切片，因此填充前 resize，读取后按实读长度截断。
失败时清空本次填充区域，此前成功读取的数据由局部 out 移回。缓冲非空时不再
调用底层源。没有未交付数据时，足够大的标准 Read 请求直接读取到调用者切片。

split_to 不复制交付载荷，但共享化可能分配控制信息。容量不足时，
reserve 可能复用已有空间、移动未交付后缀或分配新存储。读取、初始化、增长都
不是无成本的操作，因此返回 BytesMut 不等同于全流程零复制。

长期持有 BytesMut 会固定住共享分配。很短的结果也可能使较大存储继续存活；有多个
存活引用时，后续填充不能任意复用它们覆盖的区域。释放结果后能否复用取决于引用、
容量和 bytes 的具体实现。这些因素需要与立即丢弃结果的场景分别测量。

## 官方接口与源码依据

- [std::io::BufRead](https://doc.rust-lang.org/std/io/trait.BufRead.html)
  将 fill_buf 和 consume 分开；read_until 追加到调用者 Vec，读取错误后已读前缀
  保留在该 Vec 中。这里需要跨方法恢复，所以保留状态属于读取器。
- [std::io::Read](https://doc.rust-lang.org/std/io/trait.Read.html#method.read)
  允许短读；单次读取失败不交付字节，不意味着一个已经多次成功读取的操作可以
  回退底层源。目标内存初始化要求也是本实现使用安全 resize 的原因。
- [bstr BufReadExt 源码](https://docs.rs/bstr/1.13.1/src/bstr/io.rs.html)
  的 for_byte_record_with_terminator 直接把当前缓冲中的完整段借给回调，只将末尾
  片段保存后继续读取。这个实现支持复用 BufRead 的可行性；这里提供的是逐次借用
  返回接口，没有引入该回调接口，也不依赖 bstr。
- [bytes 1.10.1 BytesMut 源码](https://docs.rs/bytes/1.10.1/src/bytes/bytes_mut.rs.html)
  的 split_to 分离互不重叠的可变视图；reserve_inner 根据剩余容量、
  偏移和共享状态选择复用、搬移或另行分配。性能结论不能只看 split_to 一步。
- [memchr](https://docs.rs/memchr/2.7.4/memchr/fn.memchr.html)
  直接提供单字节位置查找，本 crate 不自行实现字节搜索算法。

上述缓冲和存储操作有官方依据。具体类型划分、默认填充大小和错误前缀
保留策略是本 crate 的工程选择，不宣称是唯一业界惯例。全 crate 禁止 unsafe。
局部 Option<BytesMut> 中的已累积前缀由 Rust 自动析构，包括 panic 展开时的清理。
编译、测试和测量范围见 [BENCHMARKS.md](BENCHMARKS.md)。
