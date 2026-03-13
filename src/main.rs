
use std::env;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::time::Instant;

const VERSION: &str = "Sargen Tiny Server v3.0";

fn print_version() {
    println!(
        r#"
   ____                       _
  / ___| __ _ _ __ ___  _ __ | | ___  ___
 | |  _ / _` | '_ ` _ \| '_ \| |/ _ \/ __|
 | |_| | (_| | | | | | | |_) | |  __/\__ \
  \____|\__,_|_| |_| |_| .__/|_|\___||___/
                       |_|
    {}
    "#,
        VERSION
    );
}

fn send_png(stream: &mut TcpStream, png_path: &Path) {
    if let Ok(data) = fs::read(png_path) {
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: image/png\r\n\r\n",
            data.len()
        );
        let _ = stream.write(header.as_bytes());
        let _ = stream.write(&data);
    } else {
        let body = "Image not found";
        let header = format!(
            "HTTP/1.1 404 NOT FOUND\r\nContent-Length: {}\r\n\r\n{}",
            body.len(),
            body
        );
        let _ = stream.write(header.as_bytes());
    }
}

fn handle_client(mut stream: TcpStream, root: &Path, debug: bool, uptime: Instant) {
    let mut buffer = [0; 4096];
    if stream.read(&mut buffer).is_ok() {
        let request = String::from_utf8_lossy(&buffer);
        let first_line = request.lines().next().unwrap_or("");
        let mut parts = first_line.split_whitespace();
        let method = parts.next().unwrap_or("");
        let path = parts.next().unwrap_or("/");

        if debug {
            println!("[DEBUG] {} {}", method, path);
        }

        if path == "/favicon.ico" {
            let response = "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n";
            let _ = stream.write(response.as_bytes());
            return;
        }

        let full_path: PathBuf = if path == "/" {
            root.to_path_buf()
        } else {
            root.join(path.trim_start_matches('/'))
        };

        // POST upload
        if method == "POST" && path == "/upload" {
            let parts: Vec<&str> = request.split("\r\n\r\n").collect();
            if parts.len() >= 2 {
                let body = parts[1];
                let filename = "uploaded_file";
                let dest = root.join(filename);
                if let Ok(mut f) = File::create(dest) {
                    let _ = f.write_all(body.as_bytes());
                    let resp = "HTTP/1.1 200 OK\r\nContent-Length: 14\r\n\r\nUpload success";
                    let _ = stream.write(resp.as_bytes());
                    return;
                }
            }
            let resp = "HTTP/1.1 400 Bad Request\r\nContent-Length: 11\r\n\r\nUpload fail";
            let _ = stream.write(resp.as_bytes());
            return;
        }

        // serve file
        if full_path.exists() && full_path.is_file() {
            if let Ok(data) = fs::read(&full_path) {
                let content_type = match full_path.extension().and_then(|s| s.to_str()) {
                    Some("html") => "text/html",
                    Some("txt") => "text/plain",
                    Some("jpg") | Some("jpeg") => "image/jpeg",
                    Some("png") => "image/png",
                    _ => "application/octet-stream",
                };
                let header = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: {}\r\n\r\n",
                    data.len(),
                    content_type
                );
                let _ = stream.write(header.as_bytes());
                let _ = stream.write(&data);
            }
        } else if full_path.exists() && full_path.is_dir() {
            let mut entries_vec = vec![];
            if let Ok(entries) = fs::read_dir(&full_path) {
                for entry in entries.flatten() {
                    let name_os = entry.file_name();
                    let name = name_os.to_string_lossy().to_string();
                    let metadata = entry.metadata().ok();
                    let file_type = if metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false) {
                        "Folder"
                    } else {
                        "File"
                    };
                    let size = metadata.map(|m| m.len()).unwrap_or(0);
                    entries_vec.push((name, file_type.to_string(), size));
                }
            }

            let mut body = format!(
                "<h1>Directory listing: {}</h1><p>Server uptime: {:.2}s</p>",
                path,
                uptime.elapsed().as_secs_f64()
            );
            body.push_str(&format!(
                "<div style='position:absolute; top:5px; right:5px;'>Total entries: {}</div>",
                entries_vec.len()
            ));

            if path != "/" {
                let parent = Path::new(path).parent().unwrap_or(Path::new("/"));
                body.push_str(&format!("<p><a href=\"{}\">⬅ Back</a></p>", parent.display()));
            }

            body.push_str(
                "<table border=1 cellpadding=5><tr><th>Name</th><th>Type</th><th>Size</th><th>Actions</th></tr>",
            );
            for (name, file_type, size) in entries_vec {
                let href = if path.ends_with('/') {
                    format!("{}{}", path, name)
                } else if path == "/" {
                    format!("/{}", name)
                } else {
                    format!("{}/{}", path.trim_end_matches('/'), name)
                };
                let display_name = if file_type == "Folder" {
                    format!("<span style='color:green'>{}/</span>", name)
                } else {
                    format!("<span style='color:blue'>{}</span>", name)
                };
                let actions = if file_type == "File" {
                    format!(
                        "<a href=\"{}\">View</a> | <a href=\"{}\" download>Download</a>",
                        href, href
                    )
                } else {
                    "-".to_string()
                };
                body.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    display_name, file_type, size, actions
                ));
            }
            body.push_str("</table>");
            body.push_str(
                r#"<form action="/upload" method="post" enctype="multipart/form-data">
<input type="file" name="file">
<input type="submit" value="Upload">
</form>"#,
            );

            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html\r\n\r\n",
                body.len()
            );
            let _ = stream.write(header.as_bytes());
            let _ = stream.write(body.as_bytes());
        } else {
            // 404
            let png_path = root.join("images/404.png");
            send_png(&mut stream, &png_path);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() >= 2 && args[1] == "--version" {
        print_version();
        return;
    }
    if args.len() < 4 {
        println!("Usage: sargen <root_folder> <port> <debug 0|1>");
        return;
    }

    let root_dir = &args[1];
    let port = &args[2];
    let debug = args[3] == "1";

    let root_path = Path::new(root_dir);
    if !root_path.exists() || !root_path.is_dir() {
        println!("Folder '{}' tidak ditemukan atau bukan folder!", root_dir);
        return;
    }

    let addr = format!("0.0.0.0:{}", port);
    println!("Sargen Tiny Server v3.0");
    println!("Root folder: {}", root_dir);
    println!("URL: http://{}", addr);
    println!("Debug: {}", debug);

    let uptime = Instant::now();
    let listener = TcpListener::bind(addr).unwrap();
    for stream in listener.incoming() {
        if let Ok(stream) = stream {
            handle_client(stream, root_path, debug, uptime);
        }
    }
}
