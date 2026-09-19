package io.logisticos.driver.feature.profile.presentation

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import io.logisticos.driver.core.network.service.AddendumDto
import io.logisticos.driver.core.network.service.HomeCatalogueItemDto
import io.logisticos.driver.core.network.service.HomeMoveApiService
import io.logisticos.driver.core.network.service.HomeMoveDto
import io.logisticos.driver.core.network.service.HomeRoomDto
import io.logisticos.driver.core.network.service.SurveyResult
import kotlinx.coroutines.async
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import retrofit2.HttpException
import javax.inject.Inject

data class SurveyUiState(
    val loading: Boolean = true,
    val error: String? = null,
    val move: HomeMoveDto? = null,
    val leadName: String? = null,
    /** The lead's pay for the move and for this survey; null for staff builds that aren't sent it. */
    val pay: io.logisticos.driver.core.network.service.HomePayDto? = null,
    val rooms: List<HomeRoomDto> = emptyList(),
    /** The latest addendum on this move, before this survey. */
    val addendum: AddendumDto? = null,
    val roomKey: String? = null,
    val lines: List<SurveyLine> = emptyList(),
    val extras: List<ExtraDraft> = emptyList(),
    val note: String = "",
    val submitting: Boolean = false,
    val submitError: String? = null,
    /** Set once the survey is in. */
    val result: SurveyResult? = null,
    val approving: Boolean = false,
    val approveError: String? = null,
    /** Approved on this phone: where the customer pays. */
    val checkoutUrl: String? = null,
) {
    /** The addendum still waiting on the customer — this survey's or an earlier one's. */
    val pendingAddendum: AddendumDto?
        get() = (result?.addendum ?: addendum)?.takeIf { it.status == "pending" && checkoutUrl == null }

    val problem: String? get() = surveyProblem(lines, extras)

    /** An earlier addendum still waits on the customer; the server refuses another. */
    val waitingOnCustomer: Boolean get() = addendum?.status == "pending"

    val room: HomeRoomDto? get() = rooms.firstOrNull { it.key == roomKey } ?: rooms.firstOrNull()

    val canSubmit: Boolean get() = move != null && !submitting && result == null && !waitingOnCustomer && problem == null
}

/**
 * The lead's survey of a reserved home move: what is in the home beyond the
 * booking, and the materials and resources the move needs. It can only add —
 * the price the customer agreed never goes down — and the customer approves
 * any addition in their app before it is charged.
 */
@HiltViewModel
class SurveyViewModel @Inject constructor(
    private val api: HomeMoveApiService,
) : ViewModel() {

    private var shipmentId: String? = null

    private val _uiState = MutableStateFlow(SurveyUiState())
    val uiState: StateFlow<SurveyUiState> = _uiState.asStateFlow()

    /** Opens the survey for one move; again for the same move changes nothing. */
    fun open(id: String) {
        if (id == shipmentId) return
        shipmentId = id
        _uiState.value = SurveyUiState()
        load()
    }

    fun load() {
        val id = shipmentId ?: return
        viewModelScope.launch {
            _uiState.update { it.copy(loading = true, error = null) }
            runCatching {
                coroutineScope {
                    val detail = async { api.move(id) }
                    val addendum = async { runCatching { api.addendum(id).data }.getOrNull() }
                    val d = detail.await()
                    val catalogue = api.catalogue(d.data.property.type).data
                    Triple(d, catalogue, addendum.await())
                }
            }.onSuccess { (detail, catalogue, addendum) ->
                _uiState.update {
                    it.copy(
                        loading = false,
                        move = detail.data,
                        leadName = detail.leadName,
                        pay = detail.pay,
                        rooms = catalogue.rooms,
                        addendum = addendum,
                        roomKey = it.roomKey ?: catalogue.rooms.firstOrNull()?.key,
                    )
                }
            }.onFailure { e ->
                _uiState.update { it.copy(loading = false, error = e.readable("Couldn't load the move")) }
            }
        }
    }

    fun selectRoom(key: String) = _uiState.update { it.copy(roomKey = key) }

    fun add(room: String, item: HomeCatalogueItemDto) = _uiState.update { it.copy(lines = addItem(it.lines, room, item)) }

    fun remove(room: String, itemKey: String) = _uiState.update { it.copy(lines = removeItem(it.lines, room, itemKey)) }

    fun toggleDismantle(room: String, itemKey: String) = _uiState.update { s ->
        s.copy(lines = s.lines.map { if (it.room == room && it.itemKey == itemKey) it.copy(dismantle = !it.dismantle) else it })
    }

    fun togglePacking(room: String, itemKey: String) = _uiState.update { s ->
        s.copy(lines = s.lines.map { if (it.room == room && it.itemKey == itemKey) it.copy(packing = !it.packing) else it })
    }

    /** Adds [extra], or answers why it can't be added. */
    fun addExtra(extra: ExtraDraft): String? {
        val problem = extraProblem(extra)
            ?: if (_uiState.value.extras.size >= SURVEY_MAX_EXTRAS) "At most $SURVEY_MAX_EXTRAS extras." else null
        if (problem == null) _uiState.update { it.copy(extras = it.extras + extra.copy(name = extra.name.trim())) }
        return problem
    }

    fun removeExtra(index: Int) = _uiState.update { s -> s.copy(extras = s.extras.filterIndexed { i, _ -> i != index }) }

    fun setNote(note: String) = _uiState.update { it.copy(note = note.take(500)) }

    /** The customer approves on this phone with the code they were sent. */
    fun approveOnSite(code: String) {
        val id = shipmentId ?: return
        val a = _uiState.value.pendingAddendum ?: return
        if (_uiState.value.approving) return
        viewModelScope.launch {
            _uiState.update { it.copy(approving = true, approveError = null) }
            runCatching { api.approveOnSite(id, a.id, io.logisticos.driver.core.network.service.OnSiteApprovalRequest(code.trim())) }
                .onSuccess { res -> _uiState.update { it.copy(approving = false, checkoutUrl = res.data.checkoutUrl ?: "") } }
                .onFailure { e ->
                    val msg = e.readable("Couldn't approve")
                    _uiState.update {
                        it.copy(
                            approving = false,
                            approveError = if (msg.contains("CODE_LOCKED")) "Too many wrong codes. The customer approves it in their own app now." else msg,
                        )
                    }
                }
        }
    }

    fun submit() {
        val s = _uiState.value
        val id = shipmentId ?: return
        if (!s.canSubmit) return
        viewModelScope.launch {
            _uiState.update { it.copy(submitting = true, submitError = null) }
            runCatching { api.submitSurvey(id, surveyRequest(s.lines, s.extras, s.note)) }
                .onSuccess { res -> _uiState.update { it.copy(submitting = false, result = res.data) } }
                .onFailure { e ->
                    val msg = if ((e as? HttpException)?.code() == 409) {
                        "An earlier addendum is still waiting on the customer. Once they answer, survey again."
                    } else {
                        e.readable("The survey didn't send")
                    }
                    _uiState.update { it.copy(submitting = false, submitError = msg) }
                }
        }
    }
}

private fun Throwable.readable(fallback: String): String =
    serverMessage((this as? HttpException)?.response()?.errorBody()?.string(), message ?: fallback)
