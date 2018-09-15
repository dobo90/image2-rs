use rayon::prelude::*;

use color::Color;
use image::Image;
use ty::Type;

pub struct Combine<'a, A: 'a + Filter, B: Filter, F: Fn(f64, f64) -> f64> {
    a: &'a A,
    b: B,
    f: F,
}

pub struct AndThen<'a, A: 'a + Filter, F: Fn(f64) -> f64> {
    a: &'a A,
    f: F,
}

impl<'a, A: Filter, F: Sync + Fn(f64) -> f64> Filter for AndThen<'a, A, F> {
    fn compute_at<T: Type, C: Color, I: Image<T, C>>(
        &self,
        x: usize,
        y: usize,
        c: usize,
        input: &[&I],
    ) -> f64 {
        let f = &self.f;
        f(self.a.compute_at(x, y, c, input))
    }
}

impl<'a, A: Filter, B: Filter, F: Sync + Fn(f64, f64) -> f64> Filter for Combine<'a, A, B, F> {
    fn compute_at<T: Type, C: Color, I: Image<T, C>>(
        &self,
        x: usize,
        y: usize,
        c: usize,
        input: &[&I],
    ) -> f64 {
        let f = &self.f;
        f(
            self.a.compute_at(x, y, c, input),
            self.b.compute_at(x, y, c, input),
        )
    }
}

pub trait Filter: Sized + Sync {
    fn compute_at<T: Type, C: Color, I: Image<T, C>>(
        &self,
        x: usize,
        y: usize,
        c: usize,
        input: &[&I],
    ) -> f64;

    fn eval_s<T: Type, C: Color, U: Type, D: Color, I: Image<T, C>, J: Image<U, D>>(
        &self,
        output: &mut I,
        input: &[&J],
    ) {
        let (width, height, channels) = output.shape();
        for y in 0..height {
            for x in 0..width {
                for c in 0..channels {
                    output.set_f(x, y, c, T::clamp(self.compute_at(x, y, c, input)));
                }
            }
        }
    }

    fn eval<
        T: Send + Type,
        C: Color,
        U: Type,
        D: Color,
        I: Sync + Send + Image<T, C>,
        J: Sync + Image<U, D>,
    >(
        &self,
        output: &mut I,
        input: &[&J],
    ) {
        let (width, _height, channels) = output.shape();

        output
            .data_mut()
            .par_iter_mut()
            .chunks(channels)
            .enumerate()
            .for_each(|(n, mut pixel)| {
                let y = n / width;
                let x = n - (y * width);
                for c in 0..channels {
                    *pixel[c] =
                        T::from_float(T::denormalize(T::clamp(self.compute_at(x, y, c, input))));
                }
            });
    }

    fn combine<A: Filter, F: Fn(f64, f64) -> f64>(&self, other: A, f: F) -> Combine<Self, A, F> {
        Combine {
            a: self,
            b: other,
            f,
        }
    }

    fn and_then<F: Fn(f64) -> f64>(&self, f: F) -> AndThen<Self, F> {
        AndThen { a: self, f }
    }
}

#[macro_export]
macro_rules! image2_filter {
    ($name:ident, $x:ident, $y:ident, $c:ident, $input:ident, $f:expr) => {
        pub struct $name;

        impl $crate::Filter for $name {
            fn compute_at<T: Type, C: Color, I: Image<T, C>>(
                &self,
                $x: usize,
                $y: usize,
                $c: usize,
                $input: &[&I],
            ) -> f64 {
                $f
            }
        }
    };
}

image2_filter!(Invert, x, y, c, input, {
    T::max_f() - input[0].get_f(x, y, c)
});

image2_filter!(Blend, x, y, c, input, {
    (input[0].get_f(x, y, c) + input[1].get_f(x, y, c)) / 2.0
});

image2_filter!(ToGrayscale, x, y, _c, input, {
    let a = input[0];
    a.get_f(x, y, 0) * 0.21 + a.get_f(x, y, 1) * 0.72 + a.get_f(x, y, 2) * 0.07
});

image2_filter!(ToColor, x, y, c, input, {
    if c == 4 && C::channels() < 4 {
        return T::max_f();
    }

    input[0].get_f(x, y, 0)
});
