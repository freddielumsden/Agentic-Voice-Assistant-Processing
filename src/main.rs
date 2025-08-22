use std::{io::Cursor, path::Ancestors, u8};
use image::{DynamicImage, ImageBuffer, ImageReader, Pixel};

struct activation_stats {
    max: u8,
    min: u8,
    activation_count: u32, // No. pixels with activation > 0
    avg_activation: f32, // Avg. activation for pixels with activation > 0
}

fn get_activation(buffer: ImageBuffer<image::Rgb<u8>, Vec<u8>>) -> ImageBuffer<image::Rgb<u8>, Vec<u8>> {
    // Creates an "activation buffer" which will store the values of
    // the "activation" - how each pixel compares to its surroundings
    let mut activation_buffer = image::RgbImage::new(buffer.width(), buffer.height());
    for (x, y, curr_pixel) in buffer.enumerate_pixels() {
        let curr_pixel_channels = curr_pixel.channels();
        let mut total_activation = 0.0;
        let mut checked_no_pixels: usize = 0;
        for x_offset  in -1..=1 {
            for y_offset in -1..=1 {
                if x_offset == 0 && y_offset == 0 {
                    continue
                }
                let offseted_x = (x as i32) + x_offset;
                let offseted_y = (y as i32) + y_offset;
                if offseted_x < 0
                    || offseted_x >= buffer.width() as i32
                    || offseted_y < 0
                    || offseted_y >= buffer.height() as i32 {
                    continue
                }
                let offseted_pixel = buffer.get_pixel(offseted_x as u32, offseted_y as u32);
                let offseted_channels = offseted_pixel.channels();
                
                let mut activation: f32 = 0.0;
                for color in 0..curr_pixel_channels.len() {
                    activation += 
                        ((curr_pixel_channels[color] as i32-offseted_channels[color] as i32) as f32).abs()
                        / curr_pixel_channels.len() as f32; // Adjusts for no. channels
                }
                total_activation += activation;
                checked_no_pixels += 1;
            }
        }
        total_activation /= checked_no_pixels as f32; // Adjusts for no. pixels around curr_pixel (finds avg)

        let activation_pixel = activation_buffer.get_pixel_mut(x, y);
        *activation_pixel = image::Rgb([total_activation as u8, total_activation as u8, total_activation as u8]);
    }
    return activation_buffer
}

fn get_activation_stats(buffer: &ImageBuffer<image::Rgb<u8>, Vec<u8>>) -> activation_stats {
    // Calculates useful statistics for a given buffer, see activation_stats
    let mut max: u8 = 0;
    let mut min: u8 = 255;
    let mut activation_count: u32 = 0;
    let mut total_activation: u32 = 0;

    for (x, y, pixel) in buffer.enumerate_pixels() {
        if pixel[0] > max {
            max = pixel[0]
        }
        if pixel[0] < min {
            min = pixel[0]
        }
        if pixel[0] > 0 {
            activation_count += 1;
            total_activation += pixel[0] as u32
        }
    }
    let avg_activation: f32 = total_activation as f32/activation_count as f32;
    return activation_stats{max, min, activation_count, avg_activation}
}
fn get_surrounding_pixels(x: u32, y: u32, width: u32, height: u32) -> Vec<(u32, u32)>{
    let mut pixels: Vec<(u32, u32)> = Vec::new();
    for x_offs in -1..=1 {
        for y_offs in -1..=1 {
            if x_offs == 0 && y_offs == 0 {
                continue
            }
            let offs_x = x as i32 + x_offs;
            let offs_y = y as i32 + y_offs;

            if offs_x < 0
                || offs_x >= width as i32
                || offs_y < 0
                || offs_y>= height as i32 {
                continue
            }
            pixels.push((offs_x as u32, offs_y as u32));
        }
    }
    return pixels;
}

// First find lines: start at some point of activation, create a vector of all points traversed
// Iterate through every point which has activation above threshold. Remove all points which have been visited
fn get_lines(buffer: &mut ImageBuffer<image::Rgb<u8>, Vec<u8>>, threshold: u8) -> Vec<Vec<(u32, u32)>> {
    // Only values above the threshold will be considered for being part of lines
    let mut lines: Vec<Vec<(u32, u32)>> = Vec::new();
    // Adds all activated pixels to lines (clusters)
    for x in 0..buffer.width() {
        for y in 0..buffer.height() {
            let pixel = buffer.get_pixel_mut(x, y);
            if pixel[0] >= threshold {
                // Start line creation
                let mut line: Vec<(u32, u32)> = vec![(x, y)]; // Reps a cluster of activated pixels
                let mut coords_to_be_checked: Vec<(u32, u32)> = vec![(x, y)]; // Stack of positions of ACTIVATED pixels which need to have surroundings searched
                
                // Deactivates starting pixel see line 
                *pixel = image::Rgb([0,0,0]);

                // While there are still activated coords which haven't had their neighbours checked
                while let Some(check_coord) = coords_to_be_checked.pop() {
                    // Immediate neighbours to current pixel being checked
                    let mut surrounding_pixels = get_surrounding_pixels(
                        check_coord.0,
                        check_coord.1,
                        buffer.width(),
                        buffer.height()
                    );
                    for pixel_index in 0..surrounding_pixels.len() {
                        let surrounding_pixel_x = surrounding_pixels[pixel_index].0;
                        let surrounding_pixel_y = surrounding_pixels[pixel_index].1;

                        let surrounding_pixel= buffer
                            .get_pixel_mut(surrounding_pixel_x, surrounding_pixel_y);
                        if surrounding_pixel.channels()[0] >= threshold {
                            coords_to_be_checked.push(surrounding_pixels[pixel_index]);

                            line.push(surrounding_pixels[pixel_index]);

                            // Ensures each pixel is only part of one line, once, by "deactivating" it
                            *surrounding_pixel = image::Rgb([0,0,0])
                        }
                    }
                }
                // Ensures few pixel lines not added
                // Likely just artefacts -> invisible, not pressable buttons
                if line.len() > 4 {
                    // Line creation finished, add line to lines
                    lines.push(line);
                }
                
            }
        }
    }
    return lines
}

struct line {
    points: Vec<(u32, u32)>,
    top_left: (u32, u32), // Basic quadrilateral
    top_right: (u32, u32),
    bottom_left: (u32, u32),
    bottom_right: (u32, u32),
    area: u32,
}

impl line {
    fn get_activation(&self) -> f32{
        return self.points.len() as f32 / self.area as f32;
    }
}

// Takes in a vector of points and inits a line which now includes extra stats
fn get_lines_stats(lines_points: Vec<Vec<(u32, u32)>>) -> Vec<line> {
    // In order to find a top left corner, create a straight y = x + c line at rightmost
    // point, then remove all points which are beneath this point. Continue to increase
    // c until only one point is left.
    // Do the same for all 4 points. -> MAY IMPLEMENT LATER
    // This process may be computationally intense. Could switch out for simpler
    // method, for example for top left point, use y of top point and x leftmost point
    let mut lines_stats: Vec<line> = Vec::new();
    for line_points in lines_points {
        let mut rightmost_point: (u32, u32) = (0, 0);
        let mut leftmost_point: (u32, u32) = (99999, 0);
        let mut top_point: (u32, u32) = (0, 0);
        let mut bottom_point: (u32, u32) = (0, 99999);
        for point in &line_points[..] {
            if point.0 > rightmost_point.0 {
                rightmost_point = (point.0, point.1)
            }
            if point.0 < leftmost_point.0 { // Can't be else if
                leftmost_point = (point.0, point.1)
            }
            if point.1 > top_point.1 {
                top_point = (point.0, point.1)
            }
            if point.1 < bottom_point.1 {
                bottom_point = (point.0, point.1)
            }
        }
        let dx = rightmost_point.0 - leftmost_point.0; // Edge - edge
        let dy = top_point.1 - bottom_point.1; // Doesn't include one of the edges
        let area: u32 = (dx+1)
            * (dy+1);

        lines_stats.push(line {
            points: line_points,
            top_left: (leftmost_point.0, top_point.1),
            top_right: (rightmost_point.0, top_point.1),
            bottom_left: (leftmost_point.0, bottom_point.1),
            bottom_right: (rightmost_point.0, bottom_point.1),
            area: area,
        })
    }
    return lines_stats;
}

const AREA_THRESHOLD: u32 = 8;
const LARGER_WIDTH_THRESHOLD: u32 = 8;
// Minimum activation relative to size
// Removes empty "box" elements.
const ACTIVATION_THRESHOLD: f32 = 0.5;

fn sanitise_lines(lines: Vec<line>) -> Vec<line> {
    let mut new_lines: Vec<line> = Vec::new();
    for line in lines {
        let width = line.top_right.0 - line.top_left.0;
        let height = line.top_left.1 - line.bottom_left.1;
        let activation: f32 = line.get_activation();
        println!("{} {} {} {} {}", width, height, line.area, line.points.len(), activation);
        if line.area >= AREA_THRESHOLD 
            && std::cmp::max(width, height) >= LARGER_WIDTH_THRESHOLD
            && activation >= ACTIVATION_THRESHOLD {
            new_lines.push(line)
        }
    }
    return new_lines
}

struct text_line {
    line: line,
    brush_colour: image::Rgb<u8>,
    text: String
}

// Returns all lines it suspects to contain text, by examining the original image
fn find_text_lines(lines: Vec<Vec<(u32, u32)>>, image: ImageBuffer<image::Rgb<u8>, Vec<u8>>) -> Vec<text_line> {
    // List containing all lines which are text
    let text_lines: Vec<text_line> = Vec::new();

    for line in lines {

    }
    return text_lines
}

//fn transcribe_text_lines(lines: Vec<Vec<(u32, u32)>>) -> Vec<Vec<(u32, u32, String)>> {}

// Takes in a line and creates an image which contains the pixels from that line's bounding box
// fn line_to_image

fn main() -> Result<(), Box<dyn std::error::Error>>{
    let img_path = "image.png";
    let img = ImageReader::open(img_path)?.decode()?;
    let buffer = DynamicImage::into_rgb8(img);
    
    let mut activation_buffer = get_activation(buffer);
    let stats = get_activation_stats(&activation_buffer);
    println!(
        "Max: {} Min: {} Activation count: {} Avg activation: {}",
        stats.max, 
        stats.min, 
        stats.activation_count, 
        stats.avg_activation
    );
    let line_threshold = 20;
    let lines = get_lines(&mut activation_buffer, line_threshold);

    let lines_stats = get_lines_stats(lines);
    let lines_stats = sanitise_lines(lines_stats);

    /* let slice = &lines_stats[..];
    for line in slice {
        for point1 in 0..line.points.len() {
            for point2 in 0..line.points.len() {
            if point1==point2 {continue};
            if line.points[point1].0 == line.points[point2].0
            && line.points[point1].1 == line.points[point2].1{
                panic!("OHNOOO")
            }
        }
        }
    } */
    let mut total_text_area = 0;
    let mut total_text_activation = 0;
    let mut n_text = 0;
    let mut line_buffer= image::RgbImage::new(activation_buffer.width(), activation_buffer.height());
    for l in 0..lines_stats.len() {
        for point in 0..lines_stats[l].points.len() {
            // println!("{} {}", lines_stats[l].points[point].0, lines_stats[l].points[point].1);
            let x = lines_stats[l].points[point].0;
            let y = lines_stats[l].points[point].1;
            let pixel = line_buffer.get_pixel_mut(x, y);
            *pixel = image::Rgb([255,255,255]);
        }
        for x in lines_stats[l].top_left.0
            ..=lines_stats[l].top_right.0 {
            let pixel = line_buffer.get_pixel_mut(x, lines_stats[l].top_left.1);
            *pixel = image::Rgb([0,255,0]);
        }
        for x in lines_stats[l].bottom_left.0
            ..=lines_stats[l].bottom_right.0 {
            let pixel = line_buffer.get_pixel_mut(x, lines_stats[l].bottom_left.1);
            *pixel = image::Rgb([0,255,0]);
        }
        for y in lines_stats[l].bottom_left.1
            ..=lines_stats[l].top_left.1 {
            let pixel = line_buffer.get_pixel_mut(lines_stats[l].top_left.0, y);
            *pixel = image::Rgb([0,255,0]);
        }
        for y in lines_stats[l].bottom_right.1
            ..=lines_stats[l].top_right.1 {
            let pixel = line_buffer.get_pixel_mut(lines_stats[l].top_right.0, y);
            *pixel = image::Rgb([0,255,0]);
        }
        line_buffer.save("line_".to_string() + img_path).unwrap();
        let mut inp = String::new();
        std::io::stdin().read_line(&mut inp).unwrap();
        if (inp == "") {
            total_text_activation += lines_stats[l].
            n_text += 1
        } else if inp == "exit" {
            break
        }
    }
    
    activation_buffer.save("new_".to_string() + img_path).unwrap();
    println!("Hello, world!");
    Ok(())
}