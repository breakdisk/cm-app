package net.cargomarket.omnideliv.courier.ui

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import net.cargomarket.omnideliv.courier.data.ComplianceApi
import net.cargomarket.omnideliv.courier.data.ProofEncoder
import net.cargomarket.omnideliv.courier.data.UploadDocumentRequest
import net.cargomarket.omnideliv.courier.domain.ChecklistItem
import net.cargomarket.omnideliv.courier.domain.ComplianceState
import net.cargomarket.omnideliv.courier.domain.DocumentPayload
import net.cargomarket.omnideliv.courier.domain.RequiredType
import net.cargomarket.omnideliv.courier.domain.SubmittedDoc
import net.cargomarket.omnideliv.courier.domain.buildChecklist
import net.cargomarket.omnideliv.courier.domain.outstandingCount
import java.io.File
import java.time.LocalDate
import javax.inject.Inject

data class ComplianceUiState(
    val loading: Boolean = true,
    val state: ComplianceState = ComplianceState.Unknown,
    val items: List<ChecklistItem> = emptyList(),
    val jurisdiction: String = "",
    /** The load failed. What is on screen is whatever loaded last, or nothing. */
    val failed: Boolean = false,
    val submitting: Boolean = false,
    val submitError: String? = null,
    /** Name of the document that just went up, for the confirmation line. */
    val justSubmitted: String? = null,
    /**
     * The encoded photo from a send that failed, kept so a retry does not ask
     * the courier to photograph their licence a second time.
     *
     * It has to be the *encoded* file, not the captured one: [ProofEncoder]
     * writes a `.webp` beside the capture and deletes the original, so the
     * `File` the camera handed back no longer exists by the time a retry runs.
     */
    val retryPhoto: File? = null,
) {
    val outstanding: Int get() = outstandingCount(items)
}

@HiltViewModel
class ComplianceViewModel @Inject constructor(
    private val api: ComplianceApi,
    private val encoder: ProofEncoder,
) : ViewModel() {

    private val _state = MutableStateFlow(ComplianceUiState())
    val state: StateFlow<ComplianceUiState> = _state.asStateFlow()

    /**
     * `LocalDate.now()` is read here and passed down, never inside
     * [buildChecklist]. The expiry rules are pure functions of a date so they
     * can be tested without waiting for one to arrive.
     */
    fun load() {
        viewModelScope.launch {
            _state.value = _state.value.copy(loading = true, failed = false)

            val result = runCatching { api.myCompliance() }
            result.fold(
                onSuccess = { res ->
                    val body = res.body()
                    if (!res.isSuccessful || body == null) {
                        _state.value = _state.value.copy(loading = false, failed = true)
                        return@fold
                    }
                    val d = body.data
                    _state.value = _state.value.copy(
                        loading = false,
                        failed = false,
                        state = ComplianceState.from(d.profile.overallStatus),
                        jurisdiction = d.profile.jurisdiction,
                        items = buildChecklist(
                            required = d.requiredTypes.map {
                                RequiredType(
                                    id = it.id,
                                    code = it.code,
                                    name = it.name,
                                    hasExpiry = it.hasExpiry,
                                    warnDaysBefore = it.warnDaysBefore,
                                )
                            },
                            documents = d.documents.map {
                                SubmittedDoc(
                                    documentTypeId = it.documentTypeId,
                                    documentNumber = it.documentNumber,
                                    status = it.status,
                                    expiryDate = it.expiryDate,
                                    rejectionReason = it.rejectionReason,
                                    submittedAt = it.submittedAt,
                                )
                            },
                            today = LocalDate.now(),
                        ),
                    )
                },
                onFailure = { _state.value = _state.value.copy(loading = false, failed = true) },
            )
        }
    }

    /**
     * Abandon whatever the last submission attempt left behind.
     *
     * Deletes the retry photo rather than merely forgetting it: a licence
     * belonging to a real person should not outlive the screen that was going
     * to send it.
     */
    fun clearSubmitFeedback() {
        _state.value.retryPhoto?.delete()
        _state.value = _state.value.copy(
            submitError = null,
            justSubmitted = null,
            retryPhoto = null,
        )
    }

    /**
     * Encode the captured photo and send it.
     *
     * **Not queued through the outbound sync worker, deliberately.** That queue
     * exists for delivery milestones, where the courier has already done the
     * work and the platform owes them money for it — losing one is losing a
     * payment. A document upload has neither property: the courier is standing
     * still, usually at home or a hub, and a failure costs them a retry rather
     * than a job. Putting it in the same queue would mean a rejected document
     * retrying forever behind the deliveries that actually matter.
     *
     * The encoded file is left on disk when a send fails and handed back as
     * [ComplianceUiState.retryPhoto], so a retry re-sends it rather than asking
     * the courier to photograph their licence a second time.
     */
    fun submit(
        typeCode: String,
        documentName: String,
        documentNumber: String,
        expiryDate: String?,
        photo: File,
        onDone: () -> Unit,
    ) {
        viewModelScope.launch {
            _state.value = _state.value.copy(submitting = true, submitError = null)

            val encoded = encoder.encode(photo)
            if (encoded == null) {
                _state.value = _state.value.copy(
                    submitting = false,
                    submitError = "That photo could not be read. Take it again.",
                )
                return@launch
            }

            val bytes = encoded.file.readBytes()
            if (!DocumentPayload.fits(bytes.size.toLong())) {
                // No retry handle: re-sending the identical file would fail the
                // identical check. A retry button here would do nothing twice.
                encoded.file.delete()
                _state.value = _state.value.copy(
                    submitting = false,
                    submitError = "That photo is too large to send. Take it again in better light.",
                )
                return@launch
            }

            val request = UploadDocumentRequest(
                documentTypeCode = typeCode,
                documentNumber = documentNumber.trim(),
                fileBase64 = DocumentPayload.encode(bytes),
                contentType = DocumentPayload.CONTENT_TYPE,
                expiryDate = expiryDate?.takeIf { it.isNotBlank() },
            )

            val result = runCatching { api.uploadDocument(request) }
            result.fold(
                onSuccess = { res ->
                    if (res.isSuccessful) {
                        // The photo is the server's now. Dropping it keeps a
                        // stranger's licence out of the cache directory for
                        // longer than it has to be there.
                        encoded.file.delete()
                        _state.value = _state.value.copy(
                            submitting = false,
                            justSubmitted = documentName,
                            retryPhoto = null,
                        )
                        // Reload so the row moves to "waiting on review" rather
                        // than the app guessing what the server did with it.
                        load()
                        onDone()
                    } else {
                        _state.value = _state.value.copy(
                            submitting = false,
                            submitError = uploadError(res.code()),
                            // A 4xx about the number or the type will fail the
                            // same way on retry; only offer one where sending
                            // again could plausibly work.
                            retryPhoto = if (retryable(res.code())) encoded.file else null,
                        )
                    }
                },
                onFailure = {
                    _state.value = _state.value.copy(
                        submitting = false,
                        submitError = "Could not reach the server. Check your signal and try again.",
                        retryPhoto = encoded.file,
                    )
                },
            )
        }
    }
}

/**
 * Turn a status code into something a courier can act on.
 *
 * Never "HTTP 413". A courier reading an error needs to know whether to retake
 * the photo, fix what they typed, or call someone — the number tells them none
 * of that.
 */
internal fun retryable(code: Int): Boolean = when (code) {
    // The payload is fine and the server is not; sending the same bytes again
    // is exactly the right move.
    in 500..599, 408, 429 -> true
    // Everything the server refused on its merits. Retrying identical bytes
    // would fail identically, and a button that does nothing twice teaches
    // couriers to distrust every button.
    else -> false
}

internal fun uploadError(code: Int): String = when (code) {
    400, 422 -> "The server would not accept that. Check the number and expiry date."
    401, 403 -> "Your session has expired. Sign out and back in."
    404 -> "That document type is not required in your area any more."
    413 -> "That photo is too large to send. Try again in better light."
    in 500..599 -> "The server had a problem. Try again in a minute."
    else -> "That did not go through. Try again."
}
