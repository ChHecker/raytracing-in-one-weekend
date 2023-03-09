//! Central struct for creating a ray tracer and rendering an image.

use crate::{
    hittable::{BoundingBoxError, Bvh, HittableListOptions},
    ppm::PPM,
    ray::Ray,
    *,
};
use image::RgbImage;
use indicatif::{ProgressBar, ProgressStyle};
use rand::Rng;
use rayon::prelude::*;

/// Central ray tracing struct.
///
/// This struct allows setting attributes of the ray tracer, creating the world, and then rendering and saving it.
///
/// # Fields
/// - `world`: World of objects. Will be created automatically and not set manually.
/// - `camera`: [`Camera`].
/// - `image_width`: Width of the resulting image.
/// - `image_height`: Height of the resulting image.
/// - `samples_per_pixel`: How many samples to take for each pixel for the purpose of anti-aliasing.
/// - `max_depth`: How often a [`Ray`] should bounce at most.
#[derive(Clone, Debug)]
pub struct Raytracer {
    pub world: HittableList,
    camera: Camera,
    image_width: u16,
    image_height: u16,
    samples_per_pixel: u16,
    max_depth: u16,
}

impl Raytracer {
    pub fn new(
        camera: Camera,
        image_width: u16,
        image_height: u16,
        samples_per_pixel: u16,
        max_depth: u16,
    ) -> Self {
        Self {
            camera,
            image_width,
            image_height,
            samples_per_pixel,
            max_depth,
            world: HittableList::new(),
        }
    }

    /// Render the image to a [`PPM`].
    ///
    /// The function [`render`](Raytracer::render) should be preferred as other image formats are much smaller and the resulting [`RgbImage`] has more possible functions.
    /// Look at [its documentation](Raytracer::render) for more details.
    pub fn render_ppm(&mut self) -> PPM {
        // Progressbar
        let bar = ProgressBar::new((self.image_height * self.image_width).try_into().unwrap());
        bar.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})",
            )
            .unwrap()
            .progress_chars("#>-"),
        );

        let colors = self.render_multithreaded_bvh(Some(&bar));

        PPM::new(colors, self.image_width, self.image_height)
    }

    /// Render to a [`RgbImage`].
    ///
    /// Tries to optimize `world` into a [`Bvh`], but falls back to the slower implementation if not possible (i.e. [`Bvh::new`] return [`BoundingBoxError`]).
    /// This function uses multithreading with the help of the [`rayon`] crate.
    pub fn render(&mut self) -> RgbImage {
        // Progressbar
        let bar = ProgressBar::new(self.image_height as u64 * self.image_width as u64);
        bar.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})",
            )
            .unwrap()
            .progress_chars("#>-"),
        );

        let colors = self.render_multithreaded_bvh(Some(&bar));

        let mut image = RgbImage::new(self.image_width.into(), self.image_height.into());
        colors.into_iter().enumerate().for_each(|(index, color)| {
            let i = index % self.image_width as usize;
            let j = index / self.image_width as usize;
            image.put_pixel(i as u32, j as u32, color.into());
        });
        image
    }

    /// Render to a [`RgbImage`] without using the optimization of [`Bvh`].
    ///
    /// Internal testing function.
    #[allow(dead_code)]
    pub(crate) fn render_without_bvh(&mut self) -> RgbImage {
        // Progressbar
        let bar = ProgressBar::new(self.image_height as u64 * self.image_width as u64);
        bar.set_style(
            ProgressStyle::with_template(
                "{spinner:.green} [{elapsed}] [{wide_bar:.cyan/blue}] {pos}/{len} ({eta})",
            )
            .unwrap()
            .progress_chars("#>-"),
        );

        let colors = self.render_multithreaded_without_bvh(Some(&bar));

        let mut image = RgbImage::new(self.image_width.into(), self.image_height.into());
        colors.into_iter().enumerate().for_each(|(index, color)| {
            let i = index % self.image_width as usize;
            let j = index / self.image_width as usize;
            image.put_pixel(i as u32, j as u32, color.into());
        });
        image
    }

    fn render_multithreaded_bvh(&mut self, bar: Option<&ProgressBar>) -> Vec<Color> {
        let bvh = match self.camera.time() {
            Some(time) => Bvh::new(self.world.clone(), time.0, time.1),
            None => Bvh::new(self.world.clone(), 0., 0.),
        };
        let world = match &bvh {
            Ok(bvh) => HittableListOptions::Bvh(bvh),
            Err(BoundingBoxError) => HittableListOptions::HittableList(&self.world),
        };

        let mut colors =
            vec![color![0., 0., 0.]; self.image_height as usize * self.image_width as usize];

        colors
            .par_iter_mut()
            .enumerate()
            .for_each(|(index, color)| {
                let mut rng = rand::thread_rng();
                let i = index % self.image_width as usize;
                let j = self.image_height as usize - index / self.image_width as usize - 1;

                let mut pixel_color = color![0., 0., 0.];

                for _ in 0..self.samples_per_pixel {
                    let u = (i as f32 + rng.gen::<f32>()) / (self.image_width - 1) as f32;
                    let v = (j as f32 + rng.gen::<f32>()) / (self.image_height - 1) as f32;
                    pixel_color +=
                        Raytracer::ray_color(&world, self.camera.get_ray(u, v), self.max_depth);
                }
                pixel_color = color!(
                    (pixel_color.r() / self.samples_per_pixel as f32).sqrt(),
                    (pixel_color.g() / self.samples_per_pixel as f32).sqrt(),
                    (pixel_color.b() / self.samples_per_pixel as f32).sqrt(),
                );

                if let Some(bar) = bar {
                    bar.inc(1);
                }

                *color = pixel_color;
            });

        colors
    }

    fn render_multithreaded_without_bvh(&mut self, bar: Option<&ProgressBar>) -> Vec<Color> {
        let mut colors =
            vec![color![0., 0., 0.]; self.image_height as usize * self.image_width as usize];

        colors
            .par_iter_mut()
            .enumerate()
            .for_each(|(index, color)| {
                let mut rng = rand::thread_rng();
                let i = index % self.image_width as usize;
                let j = self.image_height as usize - index / self.image_width as usize - 1;

                let mut pixel_color = color![0., 0., 0.];

                for _ in 0..self.samples_per_pixel {
                    let u = (i as f32 + rng.gen::<f32>()) / (self.image_width - 1) as f32;
                    let v = (j as f32 + rng.gen::<f32>()) / (self.image_height - 1) as f32;
                    pixel_color += Raytracer::ray_color_hittable(
                        &self.world,
                        self.camera.get_ray(u, v),
                        self.max_depth,
                    );
                }
                pixel_color = color!(
                    (pixel_color.r() / self.samples_per_pixel as f32).sqrt(),
                    (pixel_color.g() / self.samples_per_pixel as f32).sqrt(),
                    (pixel_color.b() / self.samples_per_pixel as f32).sqrt(),
                );

                if let Some(bar) = bar {
                    bar.inc(1);
                }

                *color = pixel_color;
            });

        colors
    }

    /// Colors the [`Ray`] according to hits when the world can be optimized as a [`Bvh`].
    fn ray_color_bvh(world: &Bvh, ray: Ray, depth: u16) -> Color {
        if depth == 0 {
            return color![0., 0., 0.];
        }

        if let Some(hit) = world.hit(ray, 0.001, f32::INFINITY) {
            if let Some((scattered, attenuation)) = hit.material().scatter(ray, hit) {
                return attenuation * Raytracer::ray_color_bvh(world, scattered, depth - 1);
            }
            return color![0., 0., 0.];
        }

        let unit_direction = ray.direction().unit_vector();
        let t = 0.5 * (unit_direction.y() + 1.0);
        (1.0 - t) * color![1., 1., 1.] + t * color![0.5, 0.7, 1.0]
    }

    /// Colors the [`Ray`] according to hits when the world cannot be optimized as a [`Bvh`].
    fn ray_color_hittable(world: &HittableList, ray: Ray, depth: u16) -> Color {
        if depth == 0 {
            return color![0., 0., 0.];
        }

        if let Some(hit) = world.hit(ray, 0.001, f32::INFINITY) {
            if let Some((scattered, attenuation)) = hit.material().scatter(ray, hit) {
                return attenuation * Raytracer::ray_color_hittable(world, scattered, depth - 1);
            }
            return color![0., 0., 0.];
        }

        let unit_direction = ray.direction().unit_vector();
        let t = 0.5 * (unit_direction.y() + 1.0);
        (1.0 - t) * color![1., 1., 1.] + t * color![0.5, 0.7, 1.0]
    }

    /// Colors the [`Ray`] according to hits.
    ///
    /// Chooses whether to use [`ray_color_bvh`] or [`ray_color_hittable`] from the [`HittableListOptions`] enum.
    fn ray_color(world: &HittableListOptions, ray: Ray, depth: u16) -> Color {
        match world {
            HittableListOptions::HittableList(world) => {
                Raytracer::ray_color_hittable(world, ray, depth)
            }
            HittableListOptions::Bvh(world) => Raytracer::ray_color_bvh(world, ray, depth),
        }
    }
}
