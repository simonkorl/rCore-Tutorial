## 线程和进程

### 基本概念

从**源代码**经过编译器一系列处理（编译、链接、优化等）得到的可执行文件，我们称为**程序（Program）**。而通俗地说，**进程（Process）**就是**正在运行**并**使用计算机资源**的程序，与放在磁盘中一动不动的程序不同：首先，进程得到了操作系统提供的**资源**：程序的代码、数据段被加载到**内存**中，程序所需的虚拟内存空间被真正构建出来。同时操作系统还给进程分配了程序所要求的各种**其他资源**，如我们上面几个章节中提到过的页表、文件的资源。

然而如果仅此而已，进程还尚未体现出其“**正在运行**”的动态特性。而正在运行意味着 **CPU** 要去执行程序代码段中的代码，为了能够进行函数调用，我们还需要**运行栈（Stack）**。

出于OS对计算机系统精细管理的目的，我们通常将“正在运行”的动态特性从进程中剥离出来，这样的一个借助 CPU 和栈的执行流，我们称之为**线程 (Thread)** 。一个进程可以有多个线程，也可以如传统进程一样只有一个线程。

这样，进程虽然仍是代表一个正在运行的程序，但是其主要功能是作为**资源的分配单位**，管理页表、文件、网络等资源。而一个进程的多个线程则共享这些资源，专注于执行，从而作为**执行的调度单位**。举一个例子，为了分配给进程一段内存，我们把一整个页表交给进程，而出于某些目的（比如为了加速需要两个线程放在两个 CPU 的核上），我们需要线程的概念来进一步细化执行的方式，这时进程内部的全部这些线程看到的就是同样的页表，看到的也是相同的地址。但是需要注意的是，这些线程为了可以独立运行，有自己的栈（会放在相同地址空间的不同位置），CPU 也会以它们这些线程为一个基本调度单位。

### 线程的表示

在不同操作系统中，为每个线程所保存的信息都不同。在这里，我们提供一种基础的实现，每个线程会包括：

- **线程 ID**：用于唯一确认一个线程，它会在系统调用等时刻用到。
- **运行栈**：每个线程都必须有一个独立的运行栈，保存运行时数据。
- **线程执行上下文**：当线程不在执行时，我们需要保存其上下文（其实就是一堆**寄存器**的值），这样之后才能够将其恢复，继续运行。和之前实现的中断一样，上下文由 `Context` 类型保存。（注：这里的**线程执行上下文**与前面提到的**中断上下文**是不同的概念）
- **所属进程的记号**：同一个进程中的多个线程，会共享页表、打开文件等信息。因此，我们将它们提取出来放到线程中。
- ***内核栈***：除了线程运行必须有的运行栈，中断处理也必须有一个单独的栈。之前，我们的中断处理是直接在原来的栈上进行（我们直接将 `Context` 压入栈）。但是在后面我们会引入用户线程，这时就只有上帝才知道发生了什么——栈指针、程序指针都可能在跨国（**国 == 特权态**）旅游。为了确保中断处理能够进行（让操作系统能够接管这样的线程），中断处理必须运行在一个准备好的、安全的栈上。这就是内核栈。不过，内核栈并没有存储在线程信息中。（注：**它的使用方法会有些复杂，我们会在后面讲解**。）

{% label %}os/src/process/thread.rs{% endlabel %}
```rust
/// 线程的信息
pub struct Thread {
    /// 线程 ID
    pub id: ThreadID,
    /// 线程的栈
    pub stack: Range<VirtualAddress>,
    /// 所属的进程
    pub process: Arc<Process>,
    /// 用 `Mutex` 包装一些可变的变量
    pub inner: Mutex<ThreadInner>,
}

/// 线程中需要可变的部分
pub struct ThreadInner {
    /// 线程执行上下文
    ///
    /// 当且仅当线程被暂停执行时，`context` 为 `Some`
    pub context: Option<Context>,
    /// 是否进入休眠
    pub sleeping: bool,
    /// 是否已经结束
    pub dead: bool,
}
```

注意到，因为线程一般使用 `Arc<Thread>` 来保存，它是不可变的，所以其中再用 `Mutex` 来包装一部分，让这部分可以修改。

### 进程的表示

在我们实现的简单操作系统中，进程只需要维护页面映射，并且存储一点额外信息：

- **用户态标识**：我们会在后面进行区分内核态线程和用户态线程。
- **访存空间 `MemorySet`**：进程中的线程会共享同一个页表，即可以访问的虚拟内存空间（简称：访存空间）。

{% label %}os/src/process/process.rs{% endlabel %}
```rust
/// 进程的信息
pub struct Process {
    /// 是否属于用户态
    pub is_user: bool,
    /// 用 `Mutex` 包装一些可变的变量
    pub inner: Mutex<ProcessInner>,
}

pub struct ProcessInner {
    /// 进程中的线程公用页表 / 内存映射
    pub memory_set: MemorySet,
//  /// 打开的文件描述符（实验五）
//  pub descriptors: Vec<Arc<dyn INode>>,
}
```

同样地，线程也需要一部分是可变的。

### 处理器

有了线程和进程，现在，我们再抽象出「处理器」来存放和管理线程池。同时，也需要存放和管理目前正在执行的线程（即中断前执行的线程，因为操作系统在工作时是处于中断、异常或系统调用服务之中）。

{% label %}os/src/process/processor.rs{% endlabel %}
```rust
/// 线程调度和管理
///
/// 休眠线程会从调度器中移除，单独保存。在它们被唤醒之前，不会被调度器安排。
pub struct Processor {
    /// 当前正在执行的线程
    current_thread: Option<Arc<Thread>>,
    /// 线程调度器，记录活跃线程
    scheduler: SchedulerImpl<Arc<Thread>>,
    /// 保存休眠线程
    sleeping_threads: HashSet<Arc<Thread>>,
}
```

- `current_thread` 需要保存当前正在运行的线程，这样当出现系统调用的时候，操作系统便可以方便地知道是哪个线程在举手。
- `scheduler` 会负责调度线程，其接口就是简单的“添加”“移除”“获取下一个”，我们会在[后面](part-6.md)详细讲到。
- 休眠线程是指等待一些外部资源（例如硬盘读取、外设读取等）的线程，这时 CPU 如果给其时间片运行是没有意义的，因此它们也就需要移出调度器而单独保存。

{% label %}os/src/process/processor.rs{% endlabel %}
```rust
lazy_static! {
    /// 全局的 [`Processor`]
    pub static ref PROCESSOR: Lock<Processor> = Lock::new(Processor::default());
}
```

注意到这里我们用了一个 `Lock`（`os/process/lock.rs`），它封装了 `spin::Mutex`，而在其基础上进一步关闭了中断。这是因为我们（以后）在内核线程中也有可能访问 `PROCESSOR`，但是此时我们不希望它被时钟打断，这样在中断处理中就无法访问 `PROCESSOR` 了，因为它已经被锁住。
