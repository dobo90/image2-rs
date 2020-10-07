use crate::*;

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Filter input
pub struct Input<'a, T: 'a + Type, C: 'a + Color> {
    /// Input images
    pub images: &'a [&'a Image<T, C>],

    /// Input pixel
    pub pixel: Option<Pixel<C>>,

    tmp: Option<Image<T, C>>,
}

impl<'a, T: 'a + Type, C: 'a + Color> Input<'a, T, C> {
    /// Create new `Input`
    pub fn new(images: &'a [&'a Image<T, C>]) -> Self {
        Input {
            images,
            pixel: None,
            tmp: None,
        }
    }

    /// Add chained pixel data
    pub fn with_pixel(mut self, pixel: Pixel<C>) -> Self {
        self.pixel = Some(pixel);
        self
    }

    /// Create a new pixel
    pub fn new_pixel(&self) -> Pixel<C> {
        Pixel::new()
    }

    /// Get number of images
    pub fn len(&self) -> usize {
        self.images.len()
    }

    /// Returns true when there are no inputs
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get input images
    pub fn images(&self) -> &[&Image<T, C>] {
        &self.images
    }

    /// Get input pixel
    ///
    /// The `image_index` parameter is used to force access to an image when data is also
    /// available. In the case of `Input::Images`, `None` translates to index 0
    pub fn get_pixel(&self, pt: impl Into<Point>, image_index: Option<usize>) -> Pixel<C> {
        let pt = pt.into();

        if let Some(tmp) = &self.tmp {
            if image_index.is_none() {
                return tmp.get_pixel(pt);
            }
        }

        match (image_index, &self.pixel) {
            (None, Some(data)) => data.clone(),
            (index, _) => self.images[index.unwrap_or_default()].get_pixel(pt),
        }
    }

    /// Get input float value
    pub fn get_f(&self, pt: impl Into<Point>, c: Channel, image_index: Option<usize>) -> f64 {
        let pt = pt.into();

        if let Some(tmp) = &self.tmp {
            if image_index.is_none() {
                return tmp.get_f(pt, c);
            }
        }

        match (image_index, &self.pixel) {
            (None, Some(data)) => data[c],
            (index, _) => self.images[index.unwrap_or_default()].get_f(pt, c),
        }
    }

    #[doc(hidden)]
    pub fn compute_intermediate_image(&mut self, size: Size, f: &impl Filter) {
        let mut dest = Image::<T, C>::new(size);
        f.eval(self.images(), &mut dest);
        self.tmp = Some(dest);
    }
}

/// Filters are used to manipulate images in a generic, composable manner
pub trait Filter: Sized + Sync {
    /// Set to true when the Filter requires an intermediate image buffer
    const REQUIRES_INTERMEDIATE_IMAGE: bool = false;

    /// Compute value of filter at a single point and channel
    fn compute_at(
        &self,
        pt: Point,
        input: &Input<impl Type, impl Color>,
        dest: &mut DataMut<impl Type, impl Color>,
    );

    /// Called before any computation takes place, this is used by `Then` to compute an
    /// intermediate image for filters wher `REQUIRES_INTERMEDIATE_IMAGE` is set to `true`
    fn before_compute(
        &self,
        _input: &mut Input<impl Type, impl Color>,
        _output: &mut Image<impl Type, impl Color>,
    ) {
    }

    /// Compute value of filter at a single point and channel with another filter pre-applied
    fn compute_at_with_filter(
        &self,
        pt: Point,
        input: &Input<impl Type, impl Color>,
        dest: &mut DataMut<impl Type, impl Color>,
        f: &impl Filter,
    ) {
        let mut px = Pixel::new();
        f.compute_at(pt, input, &mut px.data_mut());
        self.compute_at(pt, &Input::new(input.images()).with_pixel(px), dest)
    }

    /// Evaluate a filter on part of an image
    fn eval_partial<A: Type, B: Type, C: Color, D: Color>(
        &self,
        roi: Region,
        input: &[&Image<B, impl Color>],
        output: &mut Image<A, impl Color>,
    ) {
        let mut input = Input::new(input);

        self.before_compute(&mut input, output);

        let iter = output.iter_region_mut(roi);
        iter.for_each(|(pt, mut data)| {
            self.compute_at(pt, &input, &mut data);
        });
    }

    /// Evaluate filter on part of an image using the same image for input and output
    fn eval_partial_in_place<C: Color>(&self, roi: Region, output: &mut Image<impl Type, C>) {
        let input = output as *mut _ as *const _;
        let input = unsafe { &[&*input] };

        let mut input = Input::new(input);

        self.before_compute(&mut input, output);

        output.iter_region_mut(roi).for_each(|(pt, mut data)| {
            self.compute_at(pt, &input, &mut data);
        });
    }

    /// Evaluate filter in parallel
    fn eval<C: Color>(
        &self,
        input: &[&Image<impl Type, impl Color>],
        output: &mut Image<impl Type, C>,
    ) {
        let mut input = Input::new(input);

        self.before_compute(&mut input, output);
        output.for_each(|pt, mut data| {
            self.compute_at(pt, &input, &mut data);
        });
    }

    /// Evaluate filter using the same image for input and output
    fn eval_in_place<C: Color>(&self, output: &mut Image<impl Type, C>) {
        let input = output as *mut _ as *const _;
        let input = unsafe { &[&*input] };

        let mut input = Input::new(input);

        self.before_compute(&mut input, output);

        output.for_each(|pt, mut data| {
            self.compute_at(pt, &input, &mut data);
        });
    }

    /// Perform one filter then another
    fn then<B: Filter>(self, other: B) -> Then<Self, B> {
        Then { a: self, b: other }
    }

    /// Join two filters using a function
    fn join<B: Filter, F: Fn(Point, Pixel<Rgb>, Pixel<Rgb>) -> Pixel<Rgb>>(
        self,
        other: B,
        f: F,
    ) -> Join<Self, B, F> {
        Join {
            a: self,
            b: other,
            f,
        }
    }

    /// Convert filter to `AsyncFilter`
    fn to_async<'a, T: Type, C: Color, U: Type, D: Color>(
        &'a self,
        mode: AsyncMode,
        input: Input<'a, T, C>,
        output: &'a mut Image<U, D>,
    ) -> AsyncFilter<'a, Self, T, C, U, D> {
        AsyncFilter {
            mode,
            filter: self,
            input,
            output,
            x: 0,
            y: 0,
        }
    }
}

/// Executes `a` then `b`, this should not be used with kernels and transforms
pub struct Then<A: Filter, B: Filter> {
    a: A,
    b: B,
}

impl<A: Filter, B: Filter> Filter for Then<A, B> {
    fn before_compute(
        &self,
        input: &mut Input<impl Type, impl Color>,
        output: &mut Image<impl Type, impl Color>,
    ) {
        if B::REQUIRES_INTERMEDIATE_IMAGE {
            input.compute_intermediate_image(output.size(), &self.a)
        }
    }

    fn compute_at(
        &self,
        pt: Point,
        input: &Input<impl Type, impl Color>,
        dest: &mut DataMut<impl Type, impl Color>,
    ) {
        if B::REQUIRES_INTERMEDIATE_IMAGE {
            self.b.compute_at(pt, input, dest);
        } else {
            self.b
                .compute_at_with_filter(pt, &Input::new(input.images()), dest, &self.a);
        }
    }
}

/// Join two filters using the function `F`
pub struct Join<A: Filter, B: Filter, F: Fn(Point, Pixel<Rgb>, Pixel<Rgb>) -> Pixel<Rgb>> {
    a: A,
    b: B,
    f: F,
}

impl<A: Filter, B: Filter, F: Sync + Fn(Point, Pixel<Rgb>, Pixel<Rgb>) -> Pixel<Rgb>> Filter
    for Join<A, B, F>
{
    fn compute_at(
        &self,
        pt: Point,
        input: &Input<impl Type, impl Color>,
        dest: &mut DataMut<impl Type, impl Color>,
    ) {
        let mut a: Pixel<Rgb> = input.get_pixel(pt, None).convert();
        self.a.compute_at(pt, input, &mut a.data_mut());

        let mut b: Pixel<Rgb> = input.get_pixel(pt, None).convert();
        self.b.compute_at(pt, input, &mut b.data_mut());

        (self.f)(pt, a, b).copy_to_slice(dest);
    }
}

/// Saturation
pub struct Saturation(pub f64);

impl Filter for Saturation {
    fn compute_at(
        &self,
        pt: Point,
        input: &Input<impl Type, impl Color>,
        data: &mut DataMut<impl Type, impl Color>,
    ) {
        let px = input.get_pixel(pt, None);
        let mut tmp: Pixel<Hsv> = px.convert();
        tmp[1] *= self.0;
        tmp.convert_to_data(data);
    }
}

/// Adjust image brightness
pub struct Brightness(pub f64);

impl Filter for Brightness {
    fn compute_at(
        &self,
        pt: Point,
        input: &Input<impl Type, impl Color>,
        data: &mut DataMut<impl Type, impl Color>,
    ) {
        let mut px = input.get_pixel(pt, None);
        px *= self.0;
        px.convert_to_data(data);
    }
}

/// Adjust image contrast
pub struct Contrast(pub f64);

impl Filter for Contrast {
    fn compute_at(
        &self,
        pt: Point,
        input: &Input<impl Type, impl Color>,
        data: &mut DataMut<impl Type, impl Color>,
    ) {
        let mut px = input.get_pixel(pt, None);
        px.map(|x| (self.0 * (x - 0.5)) + 0.5);
        px.convert_to_data(data);
    }
}

/// Crop an image
pub struct Crop(pub Region);

impl Filter for Crop {
    fn compute_at(
        &self,
        pt: Point,
        input: &Input<impl Type, impl Color>,
        dest: &mut DataMut<impl Type, impl Color>,
    ) {
        if pt.x > self.0.point.x + self.0.size.width || pt.y > self.0.point.y + self.0.size.height {
            return;
        }

        let x = pt.x + self.0.point.x;
        let y = pt.y + self.0.point.y;
        let px = input.get_pixel((x, y), None);
        px.copy_to_slice(dest);
    }
}

/// Invert an image
pub struct Invert;

impl Filter for Invert {
    fn compute_at(
        &self,
        pt: Point,
        input: &Input<impl Type, impl Color>,
        dest: &mut DataMut<impl Type, impl Color>,
    ) {
        let mut px = input.get_pixel(pt, None);
        px.map(|x| 1.0 - x);
        px.copy_to_slice(dest);
    }
}

/// Blend two images
pub struct Blend;

impl Filter for Blend {
    fn compute_at(
        &self,
        pt: Point,
        input: &Input<impl Type, impl Color>,
        dest: &mut DataMut<impl Type, impl Color>,
    ) {
        let a = input.get_pixel(pt, None);
        let b = input.get_pixel(pt, Some(1));
        ((a + &b) / 2.).copy_to_slice(dest);
    }
}

/// Convert to log gamma
pub struct GammaLog(pub f64);

impl Default for GammaLog {
    fn default() -> GammaLog {
        GammaLog(2.2)
    }
}

impl Filter for GammaLog {
    fn compute_at(
        &self,
        pt: Point,
        input: &Input<impl Type, impl Color>,
        dest: &mut DataMut<impl Type, impl Color>,
    ) {
        let mut px = input.get_pixel(pt, None);
        px.map(|x| x.powf(1.0 / self.0));
        px.copy_to_slice(dest);
    }
}

/// Convert to linear gamma
pub struct GammaLin(pub f64);

impl Default for GammaLin {
    fn default() -> GammaLin {
        GammaLin(2.2)
    }
}

impl Filter for GammaLin {
    fn compute_at(
        &self,
        pt: Point,
        input: &Input<impl Type, impl Color>,
        dest: &mut DataMut<impl Type, impl Color>,
    ) {
        let mut px = input.get_pixel(pt, None);
        px.map(|x| x.powf(self.0));
        px.copy_to_slice(dest);
    }
}

/// AsyncMode is used to schedule the type of iteration for an `AsyncFilter`
pub enum AsyncMode {
    /// Apply to one pixel at a time
    Pixel,

    /// Apply to a row at a time
    Row,
}

impl Default for AsyncMode {
    fn default() -> AsyncMode {
        AsyncMode::Row
    }
}

/// A `Filter` that can be executed using async
pub struct AsyncFilter<'a, F: Filter, T: 'a + Type, C: Color, U: 'a + Type, D: Color = C> {
    /// Regular filter
    pub filter: &'a F,

    /// Output image
    pub output: &'a mut Image<U, D>,

    /// Input images
    pub input: Input<'a, T, C>,
    x: usize,
    y: usize,
    mode: AsyncMode,
}

impl<
        'a,
        F: Unpin + Filter,
        T: 'a + Type,
        C: Unpin + Color,
        U: 'a + Unpin + Type,
        D: Unpin + Color,
    > AsyncFilter<'a, F, T, C, U, D>
{
    /// Evaluate the filter
    pub async fn eval(self) {
        self.await
    }
}

impl<'a, F: Unpin + Filter, T: Type, C: Color, U: Unpin + Type, D: Unpin + Color>
    std::future::Future for AsyncFilter<'a, F, T, C, U, D>
{
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        ctx: &mut std::task::Context,
    ) -> std::task::Poll<Self::Output> {
        let filter = std::pin::Pin::get_mut(self);
        let width = filter.output.width();
        let height = filter.output.height();

        match filter.mode {
            AsyncMode::Row => {
                for i in 0..width {
                    let mut data = filter.output.get_mut((i, filter.y));
                    filter
                        .filter
                        .compute_at(Point::new(i, filter.y), &filter.input, &mut data);
                }
                filter.y += 1;
            }
            AsyncMode::Pixel => {
                let mut data = filter.output.get_mut((filter.x, filter.y));
                filter
                    .filter
                    .compute_at(Point::new(filter.x, filter.y), &&filter.input, &mut data);
                filter.x += 1;
                if filter.x >= width {
                    filter.x = 0;
                    filter.y += 1;
                }
            }
        }

        if filter.y < height {
            ctx.waker().wake_by_ref();
            return std::task::Poll::Pending;
        }

        std::task::Poll::Ready(())
    }
}

/// Evaluate a `Filter` as an async filter
pub async fn eval_async<'a, F: Unpin + Filter, T: Type, U: Type, C: Color, D: Color>(
    filter: &'a F,
    mode: AsyncMode,
    input: Input<'a, U, C>,
    output: &'a mut Image<T, D>,
) {
    filter.to_async(mode, input, output).await
}
