package io.logisticos.driver.feature.profile.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.network.service.ComplianceApiService
import io.logisticos.driver.core.network.service.DocumentTypeDto
import io.logisticos.driver.core.network.service.DriverDocumentDto
import io.logisticos.driver.core.network.service.ComplianceProfileDto
import io.logisticos.driver.core.network.service.UploadDocumentRequest
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import javax.inject.Inject

data class ComplianceUiState(
    val loading: Boolean = false,
    val profile: ComplianceProfileDto? = null,
    val requiredTypes: List<DocumentTypeDto> = emptyList(),
    val documents: List<DriverDocumentDto> = emptyList(),
    val uploadingTypeCode: String? = null,
    val error: String? = null,
    val lastUploadOk: Boolean = false,
) {
    /** Latest non-superseded doc per document_type_id, so the screen shows the
     *  current state for each required type rather than every historical row. */
    val latestByTypeId: Map<String, DriverDocumentDto>
        get() = documents
            .filter { it.status != "superseded" }
            .groupBy { it.documentTypeId }
            .mapValues { (_, docs) -> docs.maxByOrNull { it.submittedAt } ?: docs.first() }
}

@HiltViewModel
class ComplianceViewModel @Inject constructor(
    private val complianceApi: ComplianceApiService,
) : ViewModel() {

    private val _uiState = MutableStateFlow(ComplianceUiState())
    val uiState: StateFlow<ComplianceUiState> = _uiState.asStateFlow()

    fun load() {
        viewModelScope.launch {
            _uiState.update { it.copy(loading = true, error = null) }
            runCatching { complianceApi.getMyProfile() }
                .onSuccess { env ->
                    _uiState.update {
                        it.copy(
                            loading = false,
                            profile = env.data.profile,
                            requiredTypes = env.data.requiredTypes,
                            documents = env.data.documents,
                        )
                    }
                }
                .onFailure { e ->
                    _uiState.update {
                        it.copy(
                            loading = false,
                            error = "${e.javaClass.simpleName}: ${e.message ?: "load failed"}",
                        )
                    }
                }
        }
    }

    /**
     * Uploads a base64-encoded file for the given document_type_code.
     * Server creates the DriverDocument and supersedes any prior submission of
     * the same type, so callers don't need to track which doc to replace.
     */
    fun uploadDocument(
        typeCode: String,
        documentNumber: String,
        fileBase64: String,
        contentType: String,
        issueDate: String? = null,
        expiryDate: String? = null,
    ) {
        viewModelScope.launch {
            _uiState.update {
                it.copy(uploadingTypeCode = typeCode, error = null, lastUploadOk = false)
            }
            runCatching {
                complianceApi.uploadDocument(
                    UploadDocumentRequest(
                        documentTypeCode = typeCode,
                        documentNumber   = documentNumber.trim(),
                        fileBase64       = fileBase64,
                        contentType      = contentType,
                        issueDate        = issueDate,
                        expiryDate       = expiryDate,
                    )
                )
            }.onSuccess {
                _uiState.update { it.copy(uploadingTypeCode = null, lastUploadOk = true) }
                load()
            }.onFailure { e ->
                _uiState.update {
                    it.copy(
                        uploadingTypeCode = null,
                        error = "${e.javaClass.simpleName}: ${e.message ?: "upload failed"}",
                    )
                }
            }
        }
    }

    fun clearError() = _uiState.update { it.copy(error = null) }
}
