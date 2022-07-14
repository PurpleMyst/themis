use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use eyre::Result;
use image::{DynamicImage, GenericImage, GenericImageView, Rgba};
use indicatif::{
    ParallelProgressIterator, ProgressBar, ProgressFinish, ProgressIterator, ProgressStyle,
};
use rayon::prelude::*;
use structopt::StructOpt;

/// Calculate the average color of a given image by averaging all of its pixels together (including alpha)
fn average_color(image: &DynamicImage) -> Rgba<u8> {
    let pixel_count = image.width() as f64 * image.height() as f64;

    let (mut r, mut g, mut b, mut a) = (0., 0., 0., 0.);
    for (_x, _y, Rgba([pr, pg, pb, pa])) in image.pixels() {
        r += pr as f64 * pr as f64;
        g += pg as f64 * pg as f64;
        b += pb as f64 * pb as f64;
        a += pa as f64 * pa as f64;
    }
    let r = (r / pixel_count).sqrt() as u8;
    let g = (g / pixel_count).sqrt() as u8;
    let b = (b / pixel_count).sqrt() as u8;
    let a = (a / pixel_count).sqrt() as u8;
    Rgba([r, g, b, a])
}

/// Calculate the distance (squared) between two colors
/// Code adapted from https://stackoverflow.com/a/9085524/13204109
fn distance(Rgba([r1, g1, b1, _]): Rgba<u8>, Rgba([r2, g2, b2, _]): Rgba<u8>) -> i64 {
    let rmean = (i64::from(r1) + i64::from(r2)) / 2;
    let r = i64::from(r1) - i64::from(r2);
    let g = i64::from(g1) - i64::from(g2);
    let b = i64::from(b1) - i64::from(b2);
    (((512 + rmean) * r * r) >> 8) + 4 * g * g + (((767 - rmean) * b * b) >> 8)
}

/// Choose the image in the given tileset whose average color is closest to the given pixel
fn pick_image_for_pixel(
    pixel: Rgba<u8>,
    possible_tiles: &[(DynamicImage, Rgba<u8>)],
) -> Option<&DynamicImage> {
    possible_tiles
        .into_par_iter()
        .min_by_key(|(_img, avg)| distance(*avg, pixel))
        .map(|(img, _avg)| img)
}

/// Load the tiles from the given directory
fn load_images<P: AsRef<Path>>(dir: P, tile_side: u32) -> Result<Vec<(DynamicImage, Rgba<u8>)>> {
    let dir = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    let len = dir.len();

    Ok(dir
        .into_par_iter()
        .progress_with(make_pbar("images loaded", len as _))
        .filter_map(|entry| {
            let img = image::open(entry.path())
                .ok()?
                .thumbnail_exact(tile_side, tile_side);
            let avg = average_color(&img);
            Some((img, avg))
        })
        .collect::<Vec<_>>())
}

/// Create a styled progress bar
fn make_pbar(msg: &'static str, len: u64) -> ProgressBar {
    let bar = ProgressBar::new(len);
    bar.set_message(msg);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}/{eta_precise}] {bar:40.green/red} {pos:>4}/{len:4} {msg}")
            .progress_chars("##-")
            .on_finish(ProgressFinish::AndLeave),
    );
    bar
}

/// Create a styled spinner
fn make_spinner(msg: &'static str, done_msg: &'static str) -> ProgressBar {
    let bar = ProgressBar::new(0);
    bar.set_message(msg);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.yellow} {msg}")
            .progress_chars("##-")
            .on_finish(ProgressFinish::WithMessage(done_msg.into())),
    );
    bar.enable_steady_tick(100);
    bar
}

#[derive(StructOpt)]
struct Opt {
    /// The image to turn into a mosaic
    #[structopt(short, long, parse(from_os_str))]
    input_dir: PathBuf,

    /// The directory containing the tiles to utilize
    #[structopt(short, long, parse(from_os_str))]
    tiles_dir: PathBuf,

    /// Where to save the finished mosaic
    #[structopt(short, long, parse(from_os_str), default_value = "output")]
    output_dir: PathBuf,

    /// The side length that the target image'll be resized to.
    #[structopt(short, long, default_value = "128")]
    mosaic_size: u32,

    /// The side length of each tile.
    #[structopt(long, default_value = "26")]
    tile_size: u32,

    /// Keep the image's aspect ratio
    #[structopt(short, long)]
    keep_aspect_ratio: bool,
}

fn main() -> Result<()> {
    let Opt {
        input_dir,
        tiles_dir,
        mosaic_size,
        tile_size,
        keep_aspect_ratio,
        output_dir,
    } = Opt::from_args();

    fs::create_dir_all(&output_dir)?;

    let possible_tiles = load_images(tiles_dir, tile_size)?;

    let input_dir = fs::read_dir(input_dir)?.collect::<Result<Vec<_>, _>>()?;

    for image in input_dir {
        let input_path = image.path();
        eprintln!("Processing {}", input_path.display());

        let output = output_dir.join(format!(
            "{}.mosaic{mosaic_size}.png",
            input_path.file_stem().unwrap().to_string_lossy()
        ));
        if output.exists() {
            continue;
        }

        let img = image::open(&input_path)?;
        let img = if keep_aspect_ratio {
            img.thumbnail(mosaic_size, mosaic_size)
        } else {
            img.thumbnail_exact(mosaic_size, mosaic_size)
        };

        // For every unique pixel in the image, find its most appropiate tile
        let unique_pixels = img
            .pixels()
            .map(|(_x, _y, pixel)| pixel)
            .collect::<HashSet<_>>();
        let len = unique_pixels.len();
        let tiles = unique_pixels
            .into_par_iter()
            .progress_with(make_pbar("pixels", len as _))
            .filter_map(|pixel| {
                let tile = pick_image_for_pixel(pixel, &possible_tiles)?;
                Some((pixel, tile))
            })
            .collect::<HashMap<_, _>>();

        // Apply the mapping previously calculated and save the mosaic
        let mut mosaic = DynamicImage::new_rgba8(img.width() * tile_size, img.height() * tile_size);
        for (x, y, pixel) in img.pixels().progress_with(make_pbar(
            "actual pixels",
            u64::from(img.width() * img.height()),
        )) {
            mosaic.copy_from(&**tiles.get(&pixel).unwrap(), x * tile_size, y * tile_size)?;
        }

        let spinner = make_spinner("Saving", "Saved!");
        mosaic.save(output)?;
        spinner.finish_using_style();
    }

    Ok(())
}
