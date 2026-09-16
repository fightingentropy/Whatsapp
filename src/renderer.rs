//! Renderer selection shared by the native app and its offline measurements.

/// Native rendering backend. OpenGL stays available for comparison/recovery.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum Backend {
    /// Existing OpenGL renderer.
    #[default]
    OpenGl,
    /// Apple's Metal API through wgpu.
    #[cfg(feature = "metal")]
    Metal,
}

impl Backend {
    /// Applies only rendering options, retaining the caller's window settings.
    pub fn configure(self, options: &mut eframe::NativeOptions) {
        match self {
            Self::OpenGl => {
                options.renderer = eframe::Renderer::Glow;
                options.glow_options.vsync = false;
            }
            #[cfg(feature = "metal")]
            Self::Metal => {
                use eframe::egui_wgpu::{SurfaceConfig, WgpuSetup, WgpuSetupCreateNew};
                let mut setup = WgpuSetupCreateNew::without_display_handle();
                // Do not let WGPU_BACKEND select a different API for this fork.
                setup.instance_descriptor.backends = wgpu::Backends::METAL;
                setup.power_preference = wgpu::PowerPreference::LowPower;
                options.renderer = eframe::Renderer::Wgpu;
                options.wgpu_options.wgpu_setup = WgpuSetup::CreateNew(setup);
                options.wgpu_options.surface = SurfaceConfig {
                    present_mode: wgpu::PresentMode::AutoNoVsync,
                    desired_maximum_frame_latency: Some(1),
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_opengl_works_even_in_a_metal_build() {
        let mut options = eframe::NativeOptions::default();
        Backend::OpenGl.configure(&mut options);
        assert_eq!(options.renderer, eframe::Renderer::Glow);
        assert!(!options.glow_options.vsync);
    }

    #[cfg(feature = "metal")]
    #[test]
    fn metal_is_restricted_to_the_apple_backend() {
        let mut options = eframe::NativeOptions::default();
        Backend::Metal.configure(&mut options);
        assert_eq!(options.renderer, eframe::Renderer::Wgpu);
        let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = options.wgpu_options.wgpu_setup else {
            panic!("a new native device must be requested");
        };
        assert_eq!(setup.instance_descriptor.backends, wgpu::Backends::METAL);
        assert_eq!(
            options.wgpu_options.surface.desired_maximum_frame_latency,
            Some(1)
        );
    }
}
