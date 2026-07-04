use actix_web::{HttpResponse, web};
use std::collections::HashMap;

use crate::app_state::AppState;
use crate::error::AppError;
use ipma_data_manager::{ClearLogsRequest, DataError};

fn map_data_error(e: DataError) -> AppError {
    match e {
        DataError::Database(msg) => AppError::Internal(msg),
        DataError::Validation(msg) => AppError::Validation(msg),
        DataError::Internal(msg) => AppError::Internal(msg),
    }
}

pub async fn export_csv(
    state: web::Data<AppState>,
    type_param: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    ipma_data_manager::export_csv(state.as_ref().clone(), type_param)
        .await
        .map_err(map_data_error)
}

pub async fn import_csv(
    state: web::Data<AppState>,
    payload: actix_multipart::Multipart,
    query: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    ipma_data_manager::import_csv(state.as_ref().clone(), payload, query)
        .await
        .map_err(map_data_error)
}

pub async fn download_template(
    type_param: web::Query<HashMap<String, String>>,
) -> Result<HttpResponse, AppError> {
    ipma_data_manager::download_template(type_param)
        .await
        .map_err(map_data_error)
}

pub async fn export_database(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    ipma_data_manager::export_database(state.as_ref().clone())
        .await
        .map_err(map_data_error)
}

pub async fn clear_logs(
    state: web::Data<AppState>,
    req: web::Json<ClearLogsRequest>,
) -> Result<HttpResponse, AppError> {
    ipma_data_manager::clear_logs(state.as_ref().clone(), req)
        .await
        .map_err(map_data_error)
}

pub async fn get_logs_stats(state: web::Data<AppState>) -> Result<HttpResponse, AppError> {
    ipma_data_manager::get_logs_stats(state.as_ref().clone())
        .await
        .map_err(map_data_error)
}
