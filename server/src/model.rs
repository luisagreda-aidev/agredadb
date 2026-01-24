use arrow::datatypes::{DataType, Field, Schema};
use std::sync::Arc;

/// Crea el esquema maestro que soporta TODOS los tipos de datos requeridos.
#[allow(dead_code)]
pub fn create_universal_schema() -> Schema {
    Schema::new(vec![
        // 1. Identidad (Relacional)
        Field::new("id", DataType::Utf8, false),
        
        // 2. Metadatos (NoSQL / JSON flexible)
        // Guardados como struct o string JSON crudo optimizado
        Field::new("metadata_json", DataType::Utf8, true),

        // 3. Vector (Machine Learning / Embeddings)
        // Lista fija de 1536 floats (Compatible con OpenAI ada-002)
        Field::new("embedding", DataType::FixedSizeList(
            Arc::new(Field::new("item", DataType::Float32, true)),
            1536
        ), true),

        // 4. Contenido Binario (Reemplazo de MinIO)
        // Aquí guardamos la imagen/PDF real O una referencia al disco
        Field::new("blob_data", DataType::Binary, true),
        
        // 5. Bandera de Tipo de Almacenamiento (¿Es Inline o Referencia?)
        Field::new("is_blob_external", DataType::Boolean, false),
    ])
}
