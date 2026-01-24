use bitpacking::{BitPacker, BitPacker4x};
use wide::f32x8;

/// Motor de Compresión Turbo (SIMD Accelerated)
/// Diseñado para comprimir columnas de enteros y vectores de floats a velocidad de RAM.
pub struct TurboCompressor {
    packer: BitPacker4x, // Usa instrucciones SSE4.1/AVX2 según disponibilidad
}

impl TurboCompressor {
    pub fn new() -> Self {
        Self {
            packer: BitPacker4x::new(),
        }
    }

    /// Comprime un bloque de 128 enteros (u32) usando Bit-Packing.
    /// Esto puede reducir el tamaño de 512 bytes a ~100 bytes sin pérdida.
    pub fn compress_u32_block(&self, data: &[u32]) -> (Vec<u8>, u8) {
        assert_eq!(data.len(), BitPacker4x::BLOCK_LEN, "Data must be exactly 128 elements");
        
        // 1. Calcular el número mínimo de bits necesarios
        let num_bits = self.packer.num_bits(data);
        
        // 2. Allocar buffer de salida
        let mut compressed = vec![0u8; 4 * BitPacker4x::BLOCK_LEN];
        
        // 3. Comprimir usando SIMD
        let compressed_len = self.packer.compress(data, &mut compressed, num_bits);
        compressed.truncate(compressed_len);
        
        (compressed, num_bits)
    }

    /// Descomprime un bloque de 128 enteros (u32) en nanosegundos.
    pub fn decompress_u32_block(&self, compressed: &[u8], num_bits: u8) -> Vec<u32> {
        let mut decompressed = vec![0u32; BitPacker4x::BLOCK_LEN];
        self.packer.decompress(compressed, &mut decompressed, num_bits);
        decompressed
    }

    /// Cuantización de Vectores (Floats a Bytes) usando SIMD.
    /// Transforma f32 [-1.0, 1.0] a i8 [-127, 127] para reducir el tamaño x4.
    pub fn quantize_f32_to_i8(data: &[f32]) -> Vec<i8> {
        let mut result = Vec::with_capacity(data.len());
        
        // Procesar en bloques de 8 usando registros de 256 bits (AVX)
        for chunk in data.chunks_exact(8) {
            let simd_floats = f32x8::from(chunk);
            // Escalar a rango i8: float * 127.0
            let scaled = simd_floats * 127.0;
            let ints: [f32; 8] = scaled.into();
            
            for &f in &ints {
                result.push(f as i8);
            }
        }
        
        // Procesar remanente
        let rem = data.len() % 8;
        if rem > 0 {
            for &f in &data[data.len()-rem..] {
                result.push((f * 127.0) as i8);
            }
        }
        
        result
    }
}

/// Implementación de Delta Encoding (Diferencia entre valores sucesivos)
/// Ideal para IDs o Timestamps crecientes.
pub fn apply_delta_encoding(data: &mut [u32]) {
    if data.is_empty() { return; }
    for i in (1..data.len()).rev() {
        data[i] = data[i].wrapping_sub(data[i-1]);
    }
}

pub fn revert_delta_encoding(data: &mut [u32]) {
    if data.is_empty() { return; }
    for i in 1..data.len() {
        data[i] = data[i].wrapping_add(data[i-1]);
    }
}
