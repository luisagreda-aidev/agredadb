use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;
use sqlparser::ast::{Statement, SetExpr, Expr, BinaryOperator, Value};

#[derive(Debug)]
pub struct ParsedQuery {
    pub table: String,
    pub filter_col: Option<String>,
    pub filter_val: Option<f64>, // Simplificado para float (ej. precio)
    pub _filter_op: Option<String>, // ">", "<", "="
}

pub fn parse_sql(sql: &str) -> Result<ParsedQuery, String> {
    let dialect = GenericDialect {};
    let ast = Parser::parse_sql(&dialect, sql).map_err(|e| e.to_string())?;

    if let Statement::Query(query) = &ast[0] {
        if let SetExpr::Select(select) = &*query.body {
            // 1. Extraer Tabla
            let table = select.from[0].relation.to_string();

            // 2. Extraer WHERE (Simplificado: solo "col > val")
            if let Some(selection) = &select.selection {
                if let Expr::BinaryOp { left, op, right } = selection {
                    let col = if let Expr::Identifier(id) = &**left { id.value.clone() } else { String::new() };
                    let val = if let Expr::Value(Value::Number(n, _)) = &**right { n.parse::<f64>().unwrap_or(0.0) } else { 0.0 };
                    
                    let op_str = match op {
                        BinaryOperator::Gt => ">",
                        BinaryOperator::Lt => "<",
                        BinaryOperator::Eq => "=",
                        _ => "?"
                    }.to_string();

                    return Ok(ParsedQuery {
                        table,
                        filter_col: Some(col),
                        filter_val: Some(val),
                        _filter_op: Some(op_str),
                    });
                }
            }
            
            // Query sin filtro (SELECT * FROM table)
            return Ok(ParsedQuery { table, filter_col: None, filter_val: None, _filter_op: None });
        }
    }
    Err("SQL no soportado o complejo".to_string())
}
