use crate::*;

#[cfg(feature = "parallel")]
use rayon::prelude::*;


/// Filters are used to manipulate images in a generic, composable manner
pub trait Filter: Sized + Sync {
    /// Compute value of filter at a single point and channel
    fn compute_at(&self, pt: Point, input: &[&Image<impl Type, impl Color>], dest: &mut [impl Type]);

    /// Evaluate a filter on part of an image
    fn eval_partial<A: Type, B: Type, C: Color, D: Color>(
        &self,
        roi: Region,
        input: &[&Image<B, impl Color>],
        output: &mut Image<A, impl Color>,
    ) {
        let iter =
            output.iter_region_mut(roi);

        iter.for_each(|(pt, pixel)| {
            self.compute_at(pt, input, pixel);
        });
    }

    /// Evaluate filter in parallel
    fn eval(&self, input: &[&Image<impl Type, impl Color>], output: &mut Image<impl Type, impl Color>) {
        output.for_each(|pt, pixel| {
            self.compute_at(pt, input, pixel);
        });
    }

    /// Perform one filter then another
    fn and_then<
        'a,
        E: Color,
        Y: Color,
        A: 'a + Filter,
        B: 'a + Filter,
    >(
        &'a self,
        other: &'a B,
    ) -> AndThen<'a, Self, B> {
        AndThen {
            a: self,
            b: other,
        }
    }

    /// Convert filter to `AsyncFilter`
    fn to_async<'a, T: Type, C: Color, U: Type, D: Color>(
        &'a self,
        mode: AsyncMode,
        input: &'a [&Image<T, C>],
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

/// Executes `a` then `b` and passes the results to `f`
pub struct AndThen<
    'a,
    A: 'a + Filter,
    B: 'a + Filter,
> {
    a: &'a A,
    b: &'a B,
}

impl<'a, A: Filter, B: Filter> Filter
    for AndThen<'a, A, B>
{
    fn compute_at(&self, pt: Point, input: &[&Image<impl Type, impl Color>], dest: &mut [impl Type]) {
        self.a.compute_at(pt, input, dest);
        self.b.compute_at(pt, input, dest);
    }
}

/// Invert an image
pub struct Invert;

impl Filter for Invert {
    fn compute_at(&self, pt: Point, input: &[&Image<impl Type, impl Color>], dest: &mut [impl Type]) {
        let mut px = input[0].get_pixel(pt);
        px.map_in_place(|x| 1.0 - x);
        px.copy_to_slice(dest);
    }
}

/// Blend two images
pub struct Blend;

impl Filter for Blend {
    fn compute_at(&self, pt: Point, input: &[&Image<impl Type, impl Color>], dest: &mut [impl Type]) {
        let a = input[0].get_pixel(pt);
        let b = input[1].get_pixel(pt);
        ((a + b) / 2.).copy_to_slice(dest);
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
    fn compute_at(&self, pt: Point, input: &[&Image<impl Type, impl Color>], dest: &mut [impl Type]) {
        let mut px = input[0].get_pixel(pt);
        px.map_in_place(|x| x.powf(1.0 / self.0));
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
    fn compute_at(&self, pt: Point, input: &[&Image<impl Type, impl Color>], dest: &mut [impl Type]) {
        let mut px = input[0].get_pixel(pt);
        px.map_in_place(|x| x.powf(self.0));
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
    pub input: &'a [&'a Image<T, C>],
    x: usize,
    y: usize,
    mode: AsyncMode
}

impl<'a, F: Unpin + Filter, T: 'a + Type, C: Unpin + Color, U: 'a  + Unpin + Type,  D: Unpin + Color>
    AsyncFilter<'a, F, T, C, U, D>
{
    /// Evaluate the filter
    pub async fn eval(self) {
        self.await
    }
}

impl<'a, F: Unpin + Filter, T: Type, C: Color, U: Unpin + Type,  D: Unpin + Color>
    std::future::Future for AsyncFilter<'a, F, T, C, U, D>
{
    type Output = ();

    fn poll(
        self: std::pin::Pin<&mut Self>,
        ctx: &mut std::task::Context,
    ) -> std::task::Poll<Self::Output> {
        let input = &self.input[0];
        let filter = std::pin::Pin::get_mut(self);


        match filter.mode {
            AsyncMode::Row => {
                for i in 0 .. input.width() {
                    let data = filter.output.get_mut((i, filter.y));
                    filter
                        .filter
                        .compute_at(Point::new(i, filter.y), &filter.input, data);
                }
                filter.y += 1;
            }
            AsyncMode::Pixel => {
                let data = filter.output.get_mut((filter.x, filter.y));
                filter
                    .filter
                    .compute_at(Point::new(filter.x, filter.y), &filter.input, data);
                filter.x += 1;
                if filter.x >= input.width() {
                    filter.x = 0;
                    filter.y += 1;
                }
            }
        }


        if filter.y < input.height() {
            ctx.waker().wake_by_ref();
            return std::task::Poll::Pending;
        }

        std::task::Poll::Ready(())
    }
}

/// Evaluate a `Filter` as an async filter
pub async fn eval_async<'a, F: Unpin + Filter, T: Type, U: Type, C: Color, D: Color>(filter: &'a F, mode: AsyncMode, input: &'a [&Image<U, C>], output: &'a mut Image<T, D>) {
    filter.to_async(mode, input, output).await
}

