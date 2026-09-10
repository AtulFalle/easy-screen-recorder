//! GPU scale of WGC BGRA frames into the encoder bounding box.
//!
//! `windows-capture` copies a top-left crop when sizes differ. We blit the
//! scaled image into that corner so the encoder still sees GPU-resident pixels.

use std::mem::ManuallyDrop;
use std::slice;

use windows::core::Interface;
use windows::Win32::Foundation::{RECT, TRUE};
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::{
    ID3DBlob, ID3DInclude, D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST,
};
use windows::Win32::Graphics::Direct3D11::{
    ID3D11Device, ID3D11DeviceContext, ID3D11PixelShader, ID3D11RenderTargetView,
    ID3D11SamplerState, ID3D11Texture2D, ID3D11VertexShader, ID3D11VideoContext, ID3D11VideoDevice,
    ID3D11VideoProcessor, ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorOutputView,
    D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_COMPARISON_NEVER,
    D3D11_FILTER_MIN_MAG_MIP_LINEAR, D3D11_SAMPLER_DESC, D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV,
    D3D11_TEXTURE2D_DESC, D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_USAGE_DEFAULT,
    D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_COLOR_SPACE,
    D3D11_VIDEO_PROCESSOR_CONTENT_DESC, D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT,
    D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0, D3D11_VIDEO_PROCESSOR_STREAM,
    D3D11_VIDEO_USAGE_PLAYBACK_NORMAL, D3D11_VIEWPORT, D3D11_VPIV_DIMENSION_TEXTURE2D,
    D3D11_VPOV_DIMENSION_TEXTURE2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT, DXGI_RATIONAL, DXGI_SAMPLE_DESC};
use windows_capture::frame::Frame;

use crate::Error;

const SCALE_HLSL: &[u8] = br#"
struct VSOut { float4 pos : SV_POSITION; float2 uv : TEXCOORD0; };
VSOut VS(uint id : SV_VertexID) {
    VSOut o;
    o.uv = float2((id << 1) & 2, id & 2);
    o.pos = float4(o.uv * float2(2.0, -2.0) + float2(-1.0, 1.0), 0.0, 1.0);
    return o;
}
Texture2D tex : register(t0);
SamplerState samp : register(s0);
float4 PS(VSOut i) : SV_Target { return tex.Sample(samp, i.uv); }
"#;

pub(super) struct GpuScaler {
    in_w: u32,
    in_h: u32,
    out_w: u32,
    out_h: u32,
    backend: Backend,
}

enum Backend {
    Video(VideoBackend),
    Shader(ShaderBackend),
}

struct VideoBackend {
    video_context: ID3D11VideoContext,
    processor: ID3D11VideoProcessor,
    enumerator: ID3D11VideoProcessorEnumerator,
    video_device: ID3D11VideoDevice,
    output: ID3D11Texture2D,
    output_view: ID3D11VideoProcessorOutputView,
    scratch: ID3D11Texture2D,
}

struct ShaderBackend {
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    sampler: ID3D11SamplerState,
    output: ID3D11Texture2D,
    rtv: ID3D11RenderTargetView,
    scratch: ID3D11Texture2D,
}

impl GpuScaler {
    pub(super) fn matches(&self, in_w: u32, in_h: u32, out_w: u32, out_h: u32) -> bool {
        self.in_w == in_w && self.in_h == in_h && self.out_w == out_w && self.out_h == out_h
    }

    pub(super) fn new(
        device: &ID3D11Device,
        in_w: u32,
        in_h: u32,
        out_w: u32,
        out_h: u32,
        format: DXGI_FORMAT,
    ) -> crate::Result<Self> {
        let backend = match VideoBackend::new(device, in_w, in_h, out_w, out_h, format) {
            Ok(video) => Backend::Video(video),
            Err(_) => Backend::Shader(ShaderBackend::new(
                device, in_w, in_h, out_w, out_h, format,
            )?),
        };
        Ok(Self {
            in_w,
            in_h,
            out_w,
            out_h,
            backend,
        })
    }

    /// Scale `frame` on the GPU and write the result into its top-left `out_w`×`out_h` region.
    pub(super) fn scale_into_frame(&self, frame: &Frame<'_>) -> crate::Result<()> {
        let context = frame.device_context();
        let dest = frame.as_raw_texture();
        match &self.backend {
            Backend::Video(video) => video.blit(context, dest, frame.as_raw_texture())?,
            Backend::Shader(shader) => shader.blit(context, dest, frame.as_raw_texture())?,
        }
        Ok(())
    }
}

impl VideoBackend {
    fn new(
        device: &ID3D11Device,
        in_w: u32,
        in_h: u32,
        out_w: u32,
        out_h: u32,
        format: DXGI_FORMAT,
    ) -> crate::Result<Self> {
        let video_device: ID3D11VideoDevice = device
            .cast()
            .map_err(|e| Error::Encoder(format!("video device: {e}")))?;
        // SAFETY: the capture device is a live D3D11 device.
        let context = unsafe { device.GetImmediateContext() }
            .map_err(|e| Error::Encoder(format!("immediate context: {e}")))?;
        let video_context: ID3D11VideoContext = context
            .cast()
            .map_err(|e| Error::Encoder(format!("video context: {e}")))?;
        let content = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL {
                Numerator: 30,
                Denominator: 1,
            },
            InputWidth: in_w,
            InputHeight: in_h,
            OutputFrameRate: DXGI_RATIONAL {
                Numerator: 30,
                Denominator: 1,
            },
            OutputWidth: out_w,
            OutputHeight: out_h,
            Usage: D3D11_VIDEO_USAGE_PLAYBACK_NORMAL,
        };
        // SAFETY: `content` is a valid descriptor on the stack for the duration of the call.
        let enumerator = unsafe { video_device.CreateVideoProcessorEnumerator(&content) }
            .map_err(|e| Error::Encoder(format!("video processor enumerator: {e}")))?;
        // SAFETY: enumerator is a live COM object created above.
        let flags = unsafe { enumerator.CheckVideoProcessorFormat(format) }
            .map_err(|e| Error::Encoder(format!("video processor format: {e}")))?;
        let input_ok = flags & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_INPUT.0 as u32 != 0;
        let output_ok = flags & D3D11_VIDEO_PROCESSOR_FORMAT_SUPPORT_OUTPUT.0 as u32 != 0;
        if !input_ok || !output_ok {
            return Err(Error::Encoder(
                "video processor does not accept this BGRA format".into(),
            ));
        }
        // SAFETY: enumerator is valid; rate conversion index 0 is the default.
        let processor = unsafe { video_device.CreateVideoProcessor(&enumerator, 0) }
            .map_err(|e| Error::Encoder(format!("video processor: {e}")))?;
        let output = create_gpu_texture(device, out_w, out_h, format)?;
        let scratch = create_gpu_texture(device, in_w, in_h, format)?;
        let output_view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
            ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
            },
        };
        let mut output_view = None;
        // SAFETY: output texture lives as long as this backend; desc is stack-allocated.
        unsafe {
            video_device.CreateVideoProcessorOutputView(
                &output,
                &enumerator,
                &output_view_desc,
                Some(&mut output_view),
            )
        }
        .map_err(|e| Error::Encoder(format!("video processor output view: {e}")))?;
        let output_view = output_view
            .ok_or_else(|| Error::Encoder("video processor output view was null".into()))?;
        let rgb = D3D11_VIDEO_PROCESSOR_COLOR_SPACE { _bitfield: 0 };
        // SAFETY: processor/context are live; color-space structs are stack values.
        unsafe {
            video_context.VideoProcessorSetOutputColorSpace(&processor, &rgb);
            video_context.VideoProcessorSetStreamColorSpace(&processor, 0, &rgb);
            let dest = RECT {
                left: 0,
                top: 0,
                right: out_w as i32,
                bottom: out_h as i32,
            };
            video_context.VideoProcessorSetStreamDestRect(
                &processor,
                0,
                true,
                Some(std::ptr::from_ref(&dest)),
            );
        }
        Ok(Self {
            video_context,
            processor,
            enumerator,
            video_device,
            output,
            output_view,
            scratch,
        })
    }

    fn blit(
        &self,
        context: &ID3D11DeviceContext,
        dest: &ID3D11Texture2D,
        source: &ID3D11Texture2D,
    ) -> crate::Result<()> {
        // Copy capture into a texture we created with video-processor bind flags.
        // SAFETY: both textures are live DEFAULT GPU resources on this device.
        unsafe {
            context.CopyResource(&self.scratch, source);
        }
        let input_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
            FourCC: 0,
            ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPIV {
                    MipSlice: 0,
                    ArraySlice: 0,
                },
            },
        };
        let mut input_view = None;
        // SAFETY: scratch is a live texture; enumerator matches the session geometry.
        unsafe {
            self.video_device.CreateVideoProcessorInputView(
                &self.scratch,
                &self.enumerator,
                &input_desc,
                Some(&mut input_view),
            )
        }
        .map_err(|e| Error::Encoder(format!("video processor input view: {e}")))?;
        let input_view = input_view
            .ok_or_else(|| Error::Encoder("video processor input view was null".into()))?;
        let mut stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: TRUE,
            pInputSurface: ManuallyDrop::new(Some(input_view)),
            ..Default::default()
        };
        // SAFETY: stream holds the input view for the duration of VideoProcessorBlt.
        let blit = unsafe {
            self.video_context.VideoProcessorBlt(
                &self.processor,
                &self.output_view,
                0,
                std::slice::from_ref(&stream),
            )
        };
        // Release the view we wrapped in ManuallyDrop so it is not leaked.
        // SAFETY: pInputSurface was initialized with Some(view) immediately above.
        drop(unsafe { ManuallyDrop::take(&mut stream.pInputSurface) });
        blit.map_err(|e| Error::Encoder(format!("video processor blit: {e}")))?;
        copy_output_to_top_left(context, dest, &self.output);
        Ok(())
    }
}

impl ShaderBackend {
    fn new(
        device: &ID3D11Device,
        in_w: u32,
        in_h: u32,
        out_w: u32,
        out_h: u32,
        format: DXGI_FORMAT,
    ) -> crate::Result<Self> {
        let vs_blob = compile_shader("VS", "vs_5_0")?;
        let ps_blob = compile_shader("PS", "ps_5_0")?;
        let mut vs = None;
        let mut ps = None;
        // SAFETY: blobs are valid compiled shader bytecode from D3DCompile.
        unsafe {
            let vs_bytes = blob_bytes(&vs_blob);
            let ps_bytes = blob_bytes(&ps_blob);
            device
                .CreateVertexShader(vs_bytes, None, Some(&mut vs))
                .map_err(|e| Error::Encoder(format!("vertex shader: {e}")))?;
            device
                .CreatePixelShader(ps_bytes, None, Some(&mut ps))
                .map_err(|e| Error::Encoder(format!("pixel shader: {e}")))?;
        }
        let vs = vs.ok_or_else(|| Error::Encoder("vertex shader was null".into()))?;
        let ps = ps.ok_or_else(|| Error::Encoder("pixel shader was null".into()))?;
        let sampler_desc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_MIN_MAG_MIP_LINEAR,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            MipLODBias: 0.0,
            MaxAnisotropy: 1,
            ComparisonFunc: D3D11_COMPARISON_NEVER,
            BorderColor: [0.0; 4],
            MinLOD: 0.0,
            MaxLOD: f32::MAX,
        };
        let mut sampler = None;
        // SAFETY: sampler_desc is a valid stack descriptor.
        unsafe {
            device
                .CreateSamplerState(&sampler_desc, Some(&mut sampler))
                .map_err(|e| Error::Encoder(format!("sampler: {e}")))?;
        }
        let sampler = sampler.ok_or_else(|| Error::Encoder("sampler was null".into()))?;
        let output = create_gpu_texture(device, out_w, out_h, format)?;
        let scratch = create_gpu_texture(device, in_w, in_h, format)?;
        let mut rtv = None;
        // SAFETY: output is a RENDER_TARGET texture we created.
        unsafe {
            device
                .CreateRenderTargetView(&output, None, Some(&mut rtv))
                .map_err(|e| Error::Encoder(format!("render target: {e}")))?;
        }
        let rtv = rtv.ok_or_else(|| Error::Encoder("render target was null".into()))?;
        Ok(Self {
            vs,
            ps,
            sampler,
            output,
            rtv,
            scratch,
        })
    }

    fn blit(
        &self,
        context: &ID3D11DeviceContext,
        dest: &ID3D11Texture2D,
        source: &ID3D11Texture2D,
    ) -> crate::Result<()> {
        // SAFETY: scratch and source are GPU textures on this device.
        unsafe {
            context.CopyResource(&self.scratch, source);
        }
        // SAFETY: `context` is the capture thread's immediate device context.
        let device = unsafe { context.GetDevice() }
            .map_err(|e| Error::Encoder(format!("shader blit device: {e}")))?;
        let mut srv = None;
        // SAFETY: scratch was created with SHADER_RESOURCE.
        unsafe {
            device
                .CreateShaderResourceView(&self.scratch, None, Some(&mut srv))
                .map_err(|e| Error::Encoder(format!("shader resource: {e}")))?;
        }
        let srv = srv.ok_or_else(|| Error::Encoder("shader resource was null".into()))?;
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: output is a live texture we created.
        unsafe {
            self.output.GetDesc(&mut desc);
        }
        let viewport = D3D11_VIEWPORT {
            TopLeftX: 0.0,
            TopLeftY: 0.0,
            Width: desc.Width as f32,
            Height: desc.Height as f32,
            MinDepth: 0.0,
            MaxDepth: 1.0,
        };
        let mut deferred = None;
        // SAFETY: deferred context is optional; flags 0 is the default.
        let used_deferred = unsafe { device.CreateDeferredContext(0, Some(&mut deferred)) }.is_ok();
        let draw_ctx = if used_deferred {
            deferred.as_ref().unwrap_or(context)
        } else {
            context
        };
        // SAFETY: draw_ctx is this device's context; all bound objects are live.
        unsafe {
            draw_ctx.OMSetRenderTargets(Some(&[Some(self.rtv.clone())]), None);
            draw_ctx.RSSetViewports(Some(&[viewport]));
            draw_ctx.IASetPrimitiveTopology(D3D11_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            draw_ctx.VSSetShader(&self.vs, None);
            draw_ctx.PSSetShader(&self.ps, None);
            draw_ctx.PSSetShaderResources(0, Some(&[Some(srv.clone())]));
            draw_ctx.PSSetSamplers(0, Some(&[Some(self.sampler.clone())]));
            draw_ctx.Draw(3, 0);
            draw_ctx.PSSetShaderResources(0, Some(&[None]));
            draw_ctx.OMSetRenderTargets(Some(&[None]), None);
        }
        if used_deferred {
            if let Some(deferred_ctx) = deferred {
                let mut list = None;
                // SAFETY: FinishCommandList on a deferred context we created.
                unsafe {
                    deferred_ctx
                        .FinishCommandList(false, Some(&mut list))
                        .map_err(|e| Error::Encoder(format!("command list: {e}")))?;
                }
                if let Some(list) = list {
                    // SAFETY: restorecontextstate=true puts the WGC immediate context back.
                    unsafe {
                        context.ExecuteCommandList(&list, true);
                    }
                }
            }
        }
        copy_output_to_top_left(context, dest, &self.output);
        Ok(())
    }
}

fn compile_shader(entry: &str, target: &str) -> crate::Result<ID3DBlob> {
    let mut blob = None;
    let mut errors = None;
    let entry_z = std::ffi::CString::new(entry).map_err(|e| Error::Encoder(e.to_string()))?;
    let target_z = std::ffi::CString::new(target).map_err(|e| Error::Encoder(e.to_string()))?;
    // SAFETY: HLSL is a static byte string; entry/target are NUL-terminated.
    let compiled = unsafe {
        D3DCompile(
            SCALE_HLSL.as_ptr().cast(),
            SCALE_HLSL.len(),
            windows::core::s!("scale.hlsl"),
            None,
            None::<&ID3DInclude>,
            windows::core::PCSTR::from_raw(entry_z.as_ptr().cast()),
            windows::core::PCSTR::from_raw(target_z.as_ptr().cast()),
            0,
            0,
            &mut blob,
            Some(&mut errors),
        )
    };
    if let Err(err) = compiled {
        let detail = errors
            .as_ref()
            .map(|e| {
                // SAFETY: error blob is a UTF-8 C string from the compiler.
                unsafe {
                    let bytes = blob_bytes(e);
                    String::from_utf8_lossy(bytes).into_owned()
                }
            })
            .unwrap_or_default();
        return Err(Error::Encoder(format!(
            "shader compile ({target}): {err} {detail}"
        )));
    }
    blob.ok_or_else(|| Error::Encoder("shader blob was null".into()))
}

unsafe fn blob_bytes(blob: &ID3DBlob) -> &[u8] {
    // SAFETY: caller guarantees `blob` is a live ID3DBlob; the pointer is valid for GetBufferSize bytes.
    unsafe { slice::from_raw_parts(blob.GetBufferPointer().cast(), blob.GetBufferSize()) }
}

fn create_gpu_texture(
    device: &ID3D11Device,
    width: u32,
    height: u32,
    format: DXGI_FORMAT,
) -> crate::Result<ID3D11Texture2D> {
    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: format,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
        CPUAccessFlags: 0,
        MiscFlags: 0,
    };
    let mut tex = None;
    // SAFETY: desc is a valid texture descriptor; device is the capture GPU.
    unsafe {
        device
            .CreateTexture2D(&desc, None, Some(&mut tex))
            .map_err(|e| Error::Encoder(format!("scale texture: {e}")))?;
    }
    tex.ok_or_else(|| Error::Encoder("scale texture was null".into()))
}

fn copy_output_to_top_left(
    context: &ID3D11DeviceContext,
    dest: &ID3D11Texture2D,
    output: &ID3D11Texture2D,
) {
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    // SAFETY: output is a live texture.
    unsafe {
        output.GetDesc(&mut desc);
    }
    let src_box = D3D11_BOX {
        left: 0,
        top: 0,
        front: 0,
        right: desc.Width,
        bottom: desc.Height,
        back: 1,
    };
    // SAFETY: dest is the current WGC frame; output holds the scaled pixels.
    unsafe {
        context.CopySubresourceRegion(dest, 0, 0, 0, 0, output, 0, Some(&src_box));
        context.Flush();
    }
}
