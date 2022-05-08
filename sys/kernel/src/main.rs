// TOE Kernel
// Copyright (c) 2021 MEG-OS project

#![no_std]
#![no_main]

use alloc::boxed::Box;
use alloc::string::*;
use alloc::vec::Vec;
use core::fmt::Write;
use core::time::Duration;
use kernel::{
    fs::FileManager, io::tty::*, mem::string::*, mem::MemoryManager, rt::RuntimeEnvironment,
    system::System, task::scheduler::*, task::*, ui::font::*, ui::terminal::Terminal, ui::text::*,
    ui::window::*, *,
};
use megstd::drawing::*;

extern crate alloc;

entry!(Shell::main);

#[used]
static mut MAIN: Shell = Shell::new();

struct Shell {
    path_ext: Vec<String>,
}

enum ParsedCmdLine {
    Empty,
    InvalidQuote,
}

impl Shell {
    const fn new() -> Self {
        Self {
            path_ext: Vec::new(),
        }
    }

    fn shared<'a>() -> &'a mut Self {
        unsafe { &mut MAIN }
    }

    fn main() {
        let shared = Self::shared();
        shared.path_ext.push("wasm".to_string());

        WindowManager::set_desktop_color(AmbiguousColor::from_rgb(0x2196F3));
        if let Ok(mut file) = FileManager::open("wall.bmp") {
            let stat = file.stat().unwrap();
            let mut vec = Vec::with_capacity(stat.len() as usize);
            file.read_to_end(&mut vec).unwrap();
            if let Some(dib) = ImageLoader::from_msdib(vec.as_slice()) {
                WindowManager::set_desktop_bitmap(&dib.as_const());
            }
        }
        WindowManager::set_pointer_visible(true);
        // Timer::sleep(Duration::from_millis(100));

        Scheduler::spawn_async(Task::new(Self::status_bar_main()));
        Scheduler::spawn_async(Task::new(Self::activity_monitor_main()));
        Scheduler::spawn_async(Task::new(Self::repl_main()));
        Scheduler::perform_tasks();
    }

    async fn repl_main() {
        System::set_stdout(Box::new(Terminal::new(80, 24)));
        let stdout = System::stdout();

        Self::invoke_command(stdout, "ver");

        writeln!(stdout, "Platform {}", System::platform(),).unwrap();
        let screen = System::main_screen();
        writeln!(
            stdout,
            "Screen {}x{} {} bit color",
            screen.width(),
            screen.height(),
            screen.color_mode(),
        )
        .unwrap();

        loop {
            write!(stdout, "# ").unwrap();
            if let Ok(cmdline) = stdout.read_line_async(120).await {
                Self::invoke_command(stdout, &cmdline);
            }
        }
    }

    fn invoke_command(stdout: &mut dyn Tty, cmdline: &str) {
        match Self::parse_cmd(&cmdline, |name, args| match name {
            "clear" | "cls" => stdout.reset().unwrap(),
            "dir" => Self::cmd_dir(args),
            "type" => Self::cmd_type(stdout, args),
            "echo" => {
                for (index, word) in args.iter().skip(1).enumerate() {
                    if index > 0 {
                        stdout.write_char(' ').unwrap();
                    }
                    stdout.write_str(word).unwrap();
                }
                stdout.write_str("\r\n").unwrap();
            }
            "ver" => {
                writeln!(stdout, "{} v{}", System::name(), System::version(),).unwrap();
            }
            "memory" => {
                let mut sb = StringBuffer::with_capacity(0x1000);
                MemoryManager::statistics(&mut sb);
                print!("{}", sb.as_str());
            }
            "open" => {
                let args = &args[1..];
                let name = args[0];
                Self::spawn(name, args, false);
            }
            _ => {
                Self::spawn(name, args, true);
            }
        }) {
            Ok(_) => {}
            Err(ParsedCmdLine::Empty) => (),
            Err(ParsedCmdLine::InvalidQuote) => {
                writeln!(stdout, "Error: Invalid quote").unwrap();
            }
        }
    }

    fn spawn(name: &str, args: &[&str], wait_until: bool) -> usize {
        Self::spawn_main(name, args, wait_until).unwrap_or_else(|| {
            let mut sb = StringBuffer::new();
            let shared = Self::shared();
            for ext in &shared.path_ext {
                sb.clear();
                write!(sb, "{}.{}", name, ext).unwrap();
                match Self::spawn_main(sb.as_str(), args, wait_until) {
                    Some(v) => return v,
                    None => (),
                }
            }
            println!("Command not found: {}", name);
            1
        })
    }

    fn spawn_main(name: &str, args: &[&str], wait_until: bool) -> Option<usize> {
        FileManager::open(name)
            .map(|mut fcb| {
                let stat = fcb.stat().unwrap();
                let file_size = stat.len() as usize;
                if file_size > 0 {
                    let mut vec = Vec::with_capacity(file_size);
                    vec.resize(file_size, 0);
                    let act_size = fcb.read(vec.as_mut_slice()).unwrap();
                    vec.resize(act_size, 0);
                    let blob = vec.as_slice();
                    if let Some(mut loader) = RuntimeEnvironment::recognize(blob) {
                        loader.option().name = name.to_string();
                        loader.option().argv = args.iter().map(|v| v.to_string()).collect();
                        match loader.load(blob) {
                            Ok(_) => {
                                let child = loader.invoke_start();
                                if wait_until {
                                    child.map(|thread| thread.join());
                                }
                            }
                            Err(_) => {
                                println!("Load error");
                                return 1;
                            }
                        }
                    } else {
                        println!("Bad executable");
                        return 1;
                    }
                }
                0
            })
            .ok()
    }

    fn cmd_dir(_args: &[&str]) {
        let dir = match FileManager::read_dir("/") {
            Ok(v) => v,
            Err(_) => return,
        };
        for dir_ent in dir {
            print!(" {:<14} ", dir_ent.name());
        }
        println!("");
    }

    fn cmd_type(stdout: &mut dyn Tty, args: &[&str]) {
        let len = 1024;
        let mut sb = Vec::with_capacity(len);
        sb.resize(len, 0);
        for path in args.iter().skip(1) {
            let mut file = match FileManager::open(path) {
                Ok(v) => v,
                Err(err) => {
                    writeln!(stdout, "{:?}", err.kind()).unwrap();
                    continue;
                }
            };
            loop {
                match file.read(sb.as_mut_slice()) {
                    Ok(0) => break,
                    Ok(size) => {
                        for b in &sb[..size] {
                            stdout.write_char(*b as char).unwrap();
                        }
                    }
                    Err(err) => {
                        writeln!(stdout, "Error: {:?}", err.kind()).unwrap();
                        break;
                    }
                }
            }
            stdout.write_str("\r\n").unwrap();
        }
    }

    fn parse_cmd<F, R>(cmdline: &str, f: F) -> Result<R, ParsedCmdLine>
    where
        F: FnOnce(&str, &[&str]) -> R,
    {
        enum CmdLinePhase {
            LeadingSpace,
            Token,
            SingleQuote,
            DoubleQuote,
        }

        if cmdline.len() == 0 {
            return Err(ParsedCmdLine::Empty);
        }
        let mut sb = StringBuffer::with_capacity(cmdline.len());
        let mut args = Vec::new();
        let mut phase = CmdLinePhase::LeadingSpace;
        sb.clear();
        for c in cmdline.chars() {
            match phase {
                CmdLinePhase::LeadingSpace => match c {
                    ' ' => (),
                    '\'' => {
                        phase = CmdLinePhase::SingleQuote;
                    }
                    '\"' => {
                        phase = CmdLinePhase::DoubleQuote;
                    }
                    _ => {
                        sb.write_char(c).unwrap();
                        phase = CmdLinePhase::Token;
                    }
                },
                CmdLinePhase::Token => match c {
                    ' ' => {
                        args.push(sb.as_str());
                        phase = CmdLinePhase::LeadingSpace;
                        sb.split();
                    }
                    _ => {
                        sb.write_char(c).unwrap();
                    }
                },
                CmdLinePhase::SingleQuote => match c {
                    '\'' => {
                        args.push(sb.as_str());
                        phase = CmdLinePhase::LeadingSpace;
                        sb.split();
                    }
                    _ => {
                        sb.write_char(c).unwrap();
                    }
                },
                CmdLinePhase::DoubleQuote => match c {
                    '\"' => {
                        args.push(sb.as_str());
                        phase = CmdLinePhase::LeadingSpace;
                        sb.split();
                    }
                    _ => {
                        sb.write_char(c).unwrap();
                    }
                },
            }
        }
        match phase {
            CmdLinePhase::LeadingSpace | CmdLinePhase::Token => (),
            CmdLinePhase::SingleQuote | CmdLinePhase::DoubleQuote => {
                return Err(ParsedCmdLine::InvalidQuote)
            }
        }
        if sb.len() > 0 {
            args.push(sb.as_str());
        }
        if args.len() > 0 {
            Ok(f(args[0], args.as_slice()))
        } else {
            Err(ParsedCmdLine::Empty)
        }
    }

    fn format_bytes(sb: &mut dyn Write, val: usize) -> core::fmt::Result {
        let kb = (val >> 10) & 0x3FF;
        let mb = (val >> 20) & 0x3FF;
        let gb = val >> 30;

        if gb >= 10 {
            // > 10G
            write!(sb, "{:4}G", gb)
        } else if gb >= 1 {
            // 1G~10G
            let mb0 = (mb * 100) >> 10;
            write!(sb, "{}.{:02}G", gb, mb0)
        } else if mb >= 100 {
            // 100M~1G
            write!(sb, "{:4}M", mb)
        } else if mb >= 10 {
            // 10M~100M
            let kb00 = (kb * 10) >> 10;
            write!(sb, "{:2}.{}M", mb, kb00)
        } else if mb >= 1 {
            // 1M~10M
            let kb0 = (kb * 100) >> 10;
            write!(sb, "{}.{:02}M", mb, kb0)
        } else if kb >= 100 {
            // 100K~1M
            write!(sb, "{:4}K", kb)
        } else if kb >= 10 {
            // 10K~100K
            let b00 = ((val & 0x3FF) * 10) >> 10;
            write!(sb, "{:2}.{}K", kb, b00)
        } else {
            // 0~10K
            write!(sb, "{:5}", val)
        }
    }

    #[allow(dead_code)]
    async fn activity_monitor_main() {
        let window_size = Size::new(280, 160);
        let bg_color = AmbiguousColor::from(IndexedColor::BLACK);
        let fg_color = AmbiguousColor::from(IndexedColor::YELLOW);
        let graph_sub_color = AmbiguousColor::from(IndexedColor::LIGHT_GREEN);
        let graph_main_color = AmbiguousColor::from(IndexedColor::YELLOW);
        let graph_border_color = AmbiguousColor::from(IndexedColor::LIGHT_GRAY);

        let window = WindowBuilder::new("Activity Monitor")
            .style_add(WindowStyle::PINCHABLE)
            .frame(Rect::new(
                -window_size.width - 8,
                -window_size.height - 8,
                window_size.width,
                window_size.height,
            ))
            .bg_color(bg_color)
            .build();
        window.show();

        let n_items = 64;
        let mut usage_cursor = 0;
        let mut usage_history = {
            let mut vec = Vec::with_capacity(n_items);
            vec.resize(n_items, u8::MAX);
            vec
        };

        let font = FontDescriptor::new(FontFamily::SmallFixed, 8).unwrap();
        let mut sb = StringBuffer::with_capacity(0x1000);

        let interval = 1000;
        window.create_timer(0, Duration::from_millis(0));
        while let Some(message) = window.get_message().await {
            match message {
                WindowMessage::Timer(_timer) => {
                    sb.clear();

                    let max_value = 1000;
                    let usage = Scheduler::usage_total();
                    usage_history[usage_cursor] =
                        (254 * usize::min(max_value, max_value - usage) / max_value) as u8;
                    usage_cursor = (usage_cursor + 1) % n_items;

                    write!(sb, "Memory ").unwrap();
                    Self::format_bytes(&mut sb, MemoryManager::total_memory_size()).unwrap();
                    write!(sb, "B, ").unwrap();
                    Self::format_bytes(&mut sb, MemoryManager::free_memory_size()).unwrap();
                    write!(sb, "B Free, ").unwrap();
                    Self::format_bytes(
                        &mut sb,
                        MemoryManager::total_memory_size()
                            - MemoryManager::free_memory_size()
                            - MemoryManager::reserved_memory_size(),
                    )
                    .unwrap();
                    writeln!(sb, "B Used").unwrap();

                    Scheduler::print_statistics(&mut sb, true);

                    window.set_needs_display();
                    window.create_timer(0, Duration::from_millis(interval));
                }
                WindowMessage::Draw => {
                    window
                        .draw(|bitmap| {
                            bitmap.fill_rect(bitmap.bounds(), window.bg_color().into());

                            {
                                let padding = 4;
                                let item_size = Size::new(n_items as isize, 32);
                                let rect =
                                    Rect::new(padding, padding, item_size.width, item_size.height);
                                // let cursor = rect.x() + rect.width() + padding;

                                let h_lines = 4;
                                let v_lines = 4;
                                for i in 1..h_lines {
                                    let point = Point::new(
                                        rect.x(),
                                        rect.y() + i * item_size.height / h_lines,
                                    );
                                    bitmap.draw_hline(point, item_size.width, graph_sub_color);
                                }
                                for i in 1..v_lines {
                                    let point = Point::new(
                                        rect.x() + i * item_size.width / v_lines,
                                        rect.y(),
                                    );
                                    bitmap.draw_vline(point, item_size.height, graph_sub_color);
                                }

                                let limit = item_size.width as usize - 2;
                                for i in 0..limit {
                                    let scale = item_size.height - 2;
                                    let value1 = usage_history
                                        [((usage_cursor + i - limit) % n_items)]
                                        as isize
                                        * scale
                                        / 255;
                                    let value2 = usage_history
                                        [((usage_cursor + i - 1 - limit) % n_items)]
                                        as isize
                                        * scale
                                        / 255;
                                    let c0 = Point::new(
                                        rect.x() + i as isize + 1,
                                        rect.y() + 1 + value1,
                                    );
                                    let c1 =
                                        Point::new(rect.x() + i as isize, rect.y() + 1 + value2);
                                    bitmap.draw_line(c0, c1, graph_main_color);
                                }
                                bitmap.draw_rect(rect, graph_border_color);
                            }

                            let rect = bitmap.bounds().insets_by(EdgeInsets::new(40, 4, 4, 4));
                            TextProcessing::draw_text(
                                bitmap,
                                sb.as_str(),
                                font,
                                rect,
                                fg_color.into(),
                                0,
                                LineBreakMode::default(),
                                TextAlignment::Left,
                                ui::text::VerticalAlignment::Top,
                            );
                        })
                        .unwrap();
                }
                _ => window.handle_default_message(message),
            }
        }
        unimplemented!()
    }

    #[allow(dead_code)]
    async fn status_bar_main() {
        const STATUS_BAR_HEIGHT: isize = 24;
        let screen_size = System::main_screen().size();
        let window_size = Size::new(screen_size.width(), STATUS_BAR_HEIGHT);
        let window_rect = Rect::new(0, 0, screen_size.width(), STATUS_BAR_HEIGHT);
        let window = WindowBuilder::new("Status")
            // .style(WindowStyle::BORDER | WindowStyle::FLOATING)
            .style(WindowStyle::FLOATING)
            .frame(window_rect)
            // .bg_color(AmbiguousColor::from_rgb(0xCCCCFF))
            .build();
        window
            .draw_in_rect(window_size.into(), |bitmap| {
                let font = FontManager::ui_font();
                let s = System::short_name();
                {
                    TextProcessing::write_str(
                        bitmap,
                        s,
                        font,
                        Point::new(9, (STATUS_BAR_HEIGHT - font.line_height()) / 2),
                        IndexedColor::BLACK.into(),
                    );
                }
            })
            .unwrap();
        window.show();
        WindowManager::add_screen_insets(EdgeInsets::new(STATUS_BAR_HEIGHT, 0, 0, 0));

        let mut sb = StringBuffer::new();

        window.create_timer(0, Duration::from_millis(0));
        while let Some(message) = window.get_message().await {
            match message {
                WindowMessage::Timer(_timer) => {
                    let time = System::system_time();
                    let interval = 1_000_000_000 - time.nanos as u64;
                    window.create_timer(0, Duration::from_nanos(interval));
                    window.set_needs_display();
                }
                WindowMessage::Draw => {
                    sb.clear();
                    let time = System::system_time();
                    let tod = time.secs % 86400;
                    let min = tod / 60 % 60;
                    let hour = tod / 3600;
                    if true {
                        let sec = tod % 60;
                        if sec % 2 == 0 {
                            write!(sb, "{:2} {:02} {:02}", hour, min, sec).unwrap();
                        } else {
                            write!(sb, "{:2}:{:02}:{:02}", hour, min, sec).unwrap();
                        };
                    } else {
                        write!(sb, "{:2}:{:02}", hour, min).unwrap();
                    }

                    let font = FontManager::system_font();
                    let clock_width = font.width_of('0') * 8;
                    let clock_rect = Rect::new(
                        window_size.width() - clock_width - 8,
                        (window_size.height() - font.line_height()) / 2,
                        clock_width,
                        font.line_height(),
                    );
                    window
                        .draw_in_rect(clock_rect, |bitmap| {
                            bitmap.fill_rect(bitmap.bounds(), window.bg_color());
                            TextProcessing::write_str(
                                bitmap,
                                sb.as_str(),
                                font,
                                Point::default(),
                                IndexedColor::BLACK.into(),
                            );
                        })
                        .unwrap();
                    window.invalidate_rect(clock_rect);
                }
                _ => window.handle_default_message(message),
            }
        }
        unimplemented!()
    }
}
