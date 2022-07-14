use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use image::{DynamicImage, GenericImage, GenericImageView, Pixel, Rgba};
use indicatif::{ParallelProgressIterator, ProgressBar, ProgressIterator, ProgressStyle};
use noisy_float::prelude::*;
use rayon::prelude::*;
use structopt::StructOpt;

/// Calculate the average color of a given image by averaging all of its pixels together (including alpha)
fn average_color(image: &DynamicImage) -> Rgba<u8> {
    let pixel_count = image.width() as f64 * image.height() as f64;

    let (mut r, mut g, mut b, mut a) = (0., 0., 0., 0.);
    for (_x, _y, Rgba([pr, pg, pb, pa])) in image.pixels() {
        r += pr as f64;
        g += pg as f64;
        b += pb as f64;
        a += pa as f64;
    }

    let r = (r / pixel_count) as u8;
    let g = (g / pixel_count) as u8;
    let b = (b / pixel_count) as u8;
    let a = (a / pixel_count) as u8;
    Rgba([r, g, b, a])
}

/// Calculate the euclidean distance between two pixels
fn distance(pixel1: Rgba<u8>, pixel2: Rgba<u8>) -> R64 {
    r64(pixel1
        .map2(&pixel2, |l, r| if l < r { r - l } else { l - r })
        .channels()
        .iter()
        .map(|&n| (n as f64).powi(2))
        .sum::<f64>()
        .sqrt())
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
fn make_pbar(msg: &str, len: u64) -> ProgressBar {
    let bar = ProgressBar::new(len);
    bar.set_message(msg);
    bar.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}/{eta_precise}] {bar:40.green/red} {pos:>4}/{len:4} {msg}")
            .progress_chars("##-"),
    );
    bar
}

#[derive(StructOpt)]
struct Opt {
    /// The image to turn into a mosaic
    #[structopt(parse(from_os_str))]
    image: PathBuf,

    /// The directory containing the tiles to utilize
    #[structopt(parse(from_os_str))]
    tiles_directory: PathBuf,

    /// Where to save the finished mosaic
    #[structopt(short, long, parse(from_os_str), default_value = "mosaic.png")]
    output: PathBuf,

    /// The side length that the target image'll be resized to.
    #[structopt(short, long, default_value = "128")]
    mosaic_size: u32,

    /// The side length of each tile.
    #[structopt(short, long, default_value = "26")]
    tile_size: u32,

    /// Keep the image's aspect ratio
    #[structopt(short, long)]
    keep_aspect_ratio: bool,
}

fn main() -> Result<()> {
    let Opt {
        image,
        tiles_directory,
        mosaic_size,
        tile_size,
        keep_aspect_ratio,
        output,
    } = Opt::from_args();

    let possible_tiles = load_images(tiles_directory, tile_size)?;
    let image = image::open(image)?;
    let image = if keep_aspect_ratio {
        image.thumbnail(mosaic_size, mosaic_size)
    } else {
        image.thumbnail_exact(mosaic_size, mosaic_size)
    };

    // For every unique pixel in the image, find its most appropiate tile
    let unique_pixels = image
        .pixels()
        .map(|(_x, _y, pixel)| pixel)
        .collect::<HashSet<_>>();
    let pbar = make_pbar("pixels", unique_pixels.len() as _);
    let tiles = unique_pixels
        .into_par_iter()
        .progress_with(pbar)
        .filter_map(|pixel| {
            let tile = pick_image_for_pixel(pixel, &possible_tiles)?;
            Some((pixel, tile))
        })
        .collect::<HashMap<_, _>>();

    // Apply the mapping previously calculated and save the mosaic
    let mut mosaic = DynamicImage::new_rgba8(image.width() * tile_size, image.height() * tile_size);
    for (x, y, pixel) in image.pixels().progress_with(make_pbar(
        "actual pixels",
        u64::from(image.width() * image.height()),
    )) {
        mosaic.copy_from(&**tiles.get(&pixel).unwrap(), x * tile_size, y * tile_size)?;
    }
    mosaic.save(output)?;

    Ok(())
}
