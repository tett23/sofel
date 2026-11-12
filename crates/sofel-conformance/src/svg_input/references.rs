use super::{BgraImage, RED, check_pixel, check_samples, rendered};

const REFERENCES: &[u8] = include_bytes!("../../../../conformance/svg/references.svg");
const EXTERNAL: &str = include_str!("../../../../conformance/svg/external.svg");
const SCRIPT: &[u8] = include_bytes!("../../../../conformance/svg/script.svg");

// For axis-aligned rectangles on whole-pixel boundaries, every inner pixel
// has full coverage. Include the complete actual bitmap so additional paint
// outside the expected rectangle cannot escape comparison.
fn check_rectangle(image: &BgraImage, [left, top, right, bottom]: [i32; 4]) -> Result<(), String> {
    for y in top..bottom {
        for x in left..right {
            check_pixel(image, x, y, RED)?;
        }
    }
    for row in 0..image.rows {
        for column in 0..image.width {
            let x = i32::try_from(i64::from(image.left) + i64::from(column))
                .map_err(|_| "column out of range")?;
            let y = i32::try_from(-i64::from(image.top) + i64::from(row))
                .map_err(|_| "row out of range")?;
            if !(left..right).contains(&x) || !(top..bottom).contains(&y) {
                check_pixel(image, x, y, [0; 4])?;
            }
        }
    }
    Ok(())
}

pub(super) fn reference_observation(case: &str, observation: &str) -> Option<Result<(), String>> {
    let glyph = match case {
        "use-transform" => 1,
        "clip-transform" => 2,
        _ => return None,
    };
    // Local corners (1,1)..(4,3) -> referenced translate(2,1) ->
    // rotate(90) -> translate(12,4) -> baseline pixels [8,10) x [7,10).
    // Applying the referenced translation after rotation yields [11,13) x [6,9).
    let check: fn(&BgraImage) -> Result<(), String> = match (case, observation) {
        ("use-transform", "expanded-geometry") => |image| check_rectangle(image, [8, 7, 10, 10]),
        (_, "transform-order") => |image| check_samples(image, &[(8, 8, RED), (11, 7, [0; 4])]),
        ("clip-transform", "inside-rgba") => |image| check_pixel(image, 8, 8, RED),
        // (5,6) lies inside the transformed outer rectangle, outside the clip.
        ("clip-transform", "outside-rgba") => |image| check_pixel(image, 5, 6, [0; 4]),
        _ => return None,
    };
    Some(rendered(REFERENCES, 2, glyph, RED).and_then(|image| check(&image)))
}

pub(super) fn restriction_observation(case: &str, observation: &str) -> Option<Result<(), String>> {
    match (case, observation) {
        ("external", "resource-requests" | "control-rgba") => Some(measure_external(observation)),
        ("script", "changed-geometry" | "control-rgba") => {
            Some(rendered(SCRIPT, 1, 1, RED).and_then(|image| {
                if observation == "changed-geometry" {
                    // The script would move the rectangle ten pixels to the right.
                    check_rectangle(&image, [4, 5, 10, 8])
                } else {
                    check_control(&image)
                }
            }))
        }
        _ => None,
    }
}

fn check_control(image: &BgraImage) -> Result<(), String> {
    check_samples(image, &[(6, 6, RED), (16, 6, [0; 4])])
}

pub(super) fn external_image() -> Result<(BgraImage, usize), String> {
    let (image, requests) = with_resource_probe(|url| {
        let document = EXTERNAL.replace("http://127.0.0.1:PORT/forbidden.svg", url);
        rendered(document.as_bytes(), 1, 1, RED)
    })?;
    Ok((
        image.map_err(|e| format!("{requests} resource requests; {e}"))?,
        requests,
    ))
}

fn measure_external(observation: &str) -> Result<(), String> {
    let (image, requests) = external_image()?;
    if observation == "resource-requests" && requests != 0 {
        return Err(format!(
            "expected 0 external resource requests, got {requests}"
        ));
    }
    // Zero requests from a skipped render do not establish compliance.
    check_control(&image)
}

// The SVG points at a fresh loopback endpoint for this observation. Answer
// forbidden requests immediately so even a synchronous image loader returns.
fn with_resource_probe<T>(render: impl FnOnce(&str) -> T) -> Result<(T, usize), String> {
    use std::io::{ErrorKind, Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc::{self, TryRecvError};
    use std::time::Duration;
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|e| format!("resource probe bind: {:?}", e.kind()))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("resource probe configuration: {:?}", e.kind()))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("resource probe address: {:?}", e.kind()))?
        .port();
    std::thread::scope(|scope| {
        let (finish, rendering) = mpsc::channel::<()>();
        let monitor = scope.spawn(move || {
            let mut requests = 0;
            loop {
                let finished = rendering.try_recv() == Err(TryRecvError::Disconnected);
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        requests += 1;
                        // Consume the request before closing the socket. A reset
                        // on close would hide the 404 from a synchronous loader.
                        stream.set_read_timeout(Some(Duration::from_secs(1))).map_err(|e| format!("resource probe timeout: {:?}", e.kind()))?;
                        let mut request = [0; 4096];
                        let _ = stream.read(&mut request);
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        if finished { return Ok(requests); }
                        std::thread::sleep(Duration::from_millis(1));
                    }
                    Err(e) => return Err(format!("resource probe accept: {:?}", e.kind())),
                }
            }
        });
        let result = render(&format!("http://127.0.0.1:{port}/forbidden.svg"));
        drop(finish);
        Ok((
            result,
            monitor
                .join()
                .map_err(|_| "resource probe thread failed")??,
        ))
    })
}

#[cfg(test)]
mod reference_tests {
    use super::*;
    #[test]
    fn i2_reference_rectangle_rejects_swapped_transform_and_clip() {
        let image = BgraImage {
            left: 8,
            top: -7,
            width: 2,
            rows: 3,
            pixels: [0, 0, 255, 255].repeat(6),
        };
        assert!(check_rectangle(&image, [8, 7, 10, 10]).is_ok());
        assert!(check_rectangle(&image, [7, 8, 10, 10]).is_err());
        let mut unclipped = image.clone();
        unclipped.width = 3;
        unclipped.pixels = [0, 0, 255, 255].repeat(9);
        assert!(check_rectangle(&unclipped, [8, 7, 10, 10]).is_err());
        assert!(check_rectangle(&BgraImage::default(), [8, 7, 10, 10]).is_err());
    }

    #[test]
    fn i2_resource_probe_counts_a_real_load_and_silence() -> Result<(), String> {
        use std::io::{Read, Write};
        use std::net::TcpStream;
        let (_, silent) = with_resource_probe(|_| ())?;
        assert_eq!(silent, 0);
        let (response, requests) = with_resource_probe(|url| -> Result<String, String> {
            let address = url
                .strip_prefix("http://")
                .and_then(|s| s.split('/').next())
                .ok_or("bad test URL")?;
            let mut socket = TcpStream::connect(address).map_err(|e| e.to_string())?;
            socket
                .write_all(b"GET /forbidden.svg HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .map_err(|e| e.to_string())?;
            let mut status = String::new();
            let _ = socket.read_to_string(&mut status);
            Ok(status)
        })?;
        assert!(response?.starts_with("HTTP/1.1 404"));
        assert_eq!(requests, 1);
        Ok(())
    }
}
