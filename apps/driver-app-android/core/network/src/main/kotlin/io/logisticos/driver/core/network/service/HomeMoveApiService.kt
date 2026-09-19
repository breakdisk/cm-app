package io.logisticos.driver.core.network.service

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import retrofit2.http.*

// ─── Whole-home moves — the lead's side ───────────────────────────────────────
//
// A home move is offered only to leads onboarded for it. Claiming one reserves
// the day (no assignment yet — that is made twelve hours before the move). The
// lead surveys the home, and the survey can only add: the customer approves
// anything more in their app before it is charged.
//
// Three services behind the gateway: dispatch holds the reservations,
// driver-ops the lead's own availability, order-intake the move and survey.

// ── Reservations (dispatch) ───────────────────────────────────────────────────

@Serializable
data class HomeReservationsResponse(val data: List<LeadMoveItem> = emptyList())

/** GET /v1/home-reservations/mine — one reserved move, soonest first. */
@Serializable
data class LeadMoveItem(
    @SerialName("shipment_id")     val shipmentId: String,
    @SerialName("tracking_number") val trackingNumber: String? = null,
    @SerialName("customer_name")   val customerName: String = "",
    val pickup: String = "",
    val dropoff: String = "",
    /** ISO-8601, UTC. */
    @SerialName("move_at")         val moveAt: String,
    /** The move's local calendar day, YYYY-MM-DD. */
    @SerialName("move_date")       val moveDate: String,
    @SerialName("survey_at")       val surveyAt: String? = null,
    val trucks: Int = 1,
    val helpers: Int = 0,
    @SerialName("crew_total")      val crewTotal: Int = 1,
    @SerialName("large_estate")    val largeEstate: Boolean = false,
    val international: Boolean = false,
    /** Already turned into the day's assignment. */
    val activated: Boolean = false,
)

// ── The lead's profile and availability (driver-ops) ──────────────────────────

@Serializable
data class ProviderResponse(val data: ProviderData)

@Serializable
data class ProviderData(
    val profile: ProviderProfileDto,
    /** YYYY-MM-DD, today onwards. */
    @SerialName("off_days") val offDays: List<String> = emptyList(),
)

/** Set by operations at onboarding, except [workingDays], which the lead keeps. */
@Serializable
data class ProviderProfileDto(
    /** "freight_move" | "home_move" */
    @SerialName("service_lines")       val serviceLines: List<String> = emptyList(),
    /** "local" | "international" */
    val coverage: List<String> = emptyList(),
    @SerialName("multi_truck_capable") val multiTruckCapable: Boolean = false,
    @SerialName("fleet_trucks")        val fleetTrucks: Int = 1,
    @SerialName("registered_helpers")  val registeredHelpers: Int = 0,
    @SerialName("max_daily_jobs")      val maxDailyJobs: Int = 1,
    /** 0 = Monday … 6 = Sunday. */
    @SerialName("working_days")        val workingDays: List<Int> = emptyList(),
)

/** PUT /v1/drivers/me/availability — both lists replaced whole. */
@Serializable
data class AvailabilityRequest(
    @SerialName("working_days") val workingDays: List<Int>,
    @SerialName("off_days")     val offDays: List<String>,
)

@Serializable
data class AvailabilityResponse(val data: AvailabilityData)

@Serializable
data class AvailabilityData(
    @SerialName("working_days") val workingDays: List<Int> = emptyList(),
    @SerialName("off_days")     val offDays: List<String> = emptyList(),
)

// ── The move, its catalogue and the survey (order-intake) ─────────────────────

@Serializable
data class HomeCatalogueResponse(val data: HomeCatalogueData)

@Serializable
data class HomeCatalogueData(
    @SerialName("property_type") val propertyType: String,
    val sizes: List<String> = emptyList(),
    val rooms: List<HomeRoomDto> = emptyList(),
    @SerialName("truck_name")    val truckName: String = "",
)

@Serializable
data class HomeRoomDto(
    val key: String,
    val name: String,
    val items: List<HomeCatalogueItemDto> = emptyList(),
)

@Serializable
data class HomeCatalogueItemDto(
    val key: String,
    val group: String = "",
    val name: String,
    @SerialName("volume_l")  val volumeL: Int = 0,
    @SerialName("weight_kg") val weightKg: Int = 0,
    /** Offered for dismantle and rebuild. */
    val assembly: Boolean = false,
    /** Needs special packing. */
    val packing: Boolean = false,
)

@Serializable
data class HomeMoveDetailResponse(
    val data: HomeMoveDto,
    @SerialName("lead_name") val leadName: String? = null,
)

@Serializable
data class HomeMoveDto(
    @SerialName("shipment_id")         val shipmentId: String,
    val property: HomePropertyDto,
    val items: List<HomePricedItemDto> = emptyList(),
    @SerialName("survey_required")     val surveyRequired: Boolean = false,
    @SerialName("survey_at")           val surveyAt: String? = null,
    @SerialName("move_at")             val moveAt: String,
    @SerialName("total_cents")         val totalCents: Long = 0,
    val currency: String = "PHP",
    val trucks: Int = 1,
    val helpers: Int = 0,
    @SerialName("crew_total")          val crewTotal: Int = 1,
    @SerialName("large_estate")        val largeEstate: Boolean = false,
    val international: Boolean = false,
    @SerialName("survey_submitted_at") val surveySubmittedAt: String? = null,
)

@Serializable
data class HomePropertyDto(
    /** "apartment" | "villa" | "offices" */
    val type: String,
    val size: String = "",
    @SerialName("pickup_floor")     val pickupFloor: Int = 0,
    @SerialName("pickup_has_lift")  val pickupHasLift: Boolean = true,
    @SerialName("dropoff_floor")    val dropoffFloor: Int = 0,
    @SerialName("dropoff_has_lift") val dropoffHasLift: Boolean = true,
    @SerialName("long_carry")       val longCarry: Boolean = false,
)

/** A booked (or surveyed) line, with the catalogue's size on it. */
@Serializable
data class HomePricedItemDto(
    val room: String,
    @SerialName("item_key") val itemKey: String,
    val name: String = "",
    val qty: Int = 1,
    @SerialName("volume_l")  val volumeL: Int = 0,
    @SerialName("weight_kg") val weightKg: Int = 0,
    val dismantle: Boolean = false,
    val packing: Boolean = false,
)

/** POST /v1/shipments/:id/home/survey — additions only. */
@Serializable
data class SurveyRequest(
    val items: List<SurveyItemDto>,
    val extras: List<SurveyExtraDto>,
    val note: String,
)

@Serializable
data class SurveyItemDto(
    val room: String,
    @SerialName("item_key") val itemKey: String,
    val qty: Int,
    val dismantle: Boolean,
    val packing: Boolean,
)

/** A packing material or a resource (an extra helper, a hoist), priced per unit. */
@Serializable
data class SurveyExtraDto(
    /** "material" | "resource" */
    val kind: String,
    val name: String,
    val qty: Int,
    @SerialName("unit_cents") val unitCents: Long,
)

@Serializable
data class SurveyResponse(val data: SurveyResult)

@Serializable
data class SurveyResult(
    @SerialName("survey_submitted") val surveySubmitted: Boolean = true,
    /** Null when the survey found nothing beyond the booking. */
    val addendum: AddendumDto? = null,
)

@Serializable
data class AddendumResponse(val data: AddendumDto? = null)

@Serializable
data class AddendumDto(
    val id: String,
    @SerialName("items_cents")  val itemsCents: Long = 0,
    @SerialName("extras_cents") val extrasCents: Long = 0,
    @SerialName("total_cents")  val totalCents: Long = 0,
    val currency: String = "PHP",
    val trucks: Int = 1,
    @SerialName("crew_total")   val crewTotal: Int = 1,
    /** "pending" | "approved" | "declined" | "paid" */
    val status: String = "pending",
    @SerialName("created_at")   val createdAt: String = "",
    @SerialName("decided_at")   val decidedAt: String? = null,
)

// ─── API interface ────────────────────────────────────────────────────────────

interface HomeMoveApiService {

    /** The lead's reserved whole-home moves — surveys to do and moves to run. */
    @GET("v1/home-reservations/mine")
    suspend fun myReservations(): HomeReservationsResponse

    /** The lead's onboarding (read-only here) and availability. 404 = not onboarded. */
    @GET("v1/drivers/me/provider")
    suspend fun myProvider(): ProviderResponse

    @PUT("v1/drivers/me/availability")
    suspend fun setAvailability(@Body body: AvailabilityRequest): AvailabilityResponse

    @GET("v1/shipments/home/catalogue")
    suspend fun catalogue(@Query("property_type") propertyType: String): HomeCatalogueResponse

    /** The move as booked. Readable by the lead who reserved it. */
    @GET("v1/shipments/{id}/home")
    suspend fun move(@Path("id") shipmentId: String): HomeMoveDetailResponse

    /** The latest addendum, if the survey opened one. */
    @GET("v1/shipments/{id}/home/addendum")
    suspend fun addendum(@Path("id") shipmentId: String): AddendumResponse

    /**
     * 201 with an addendum when the survey found more than was booked; 200
     * with none when it did not. 409 while an earlier addendum still waits on
     * the customer.
     */
    @POST("v1/shipments/{id}/home/survey")
    suspend fun submitSurvey(@Path("id") shipmentId: String, @Body body: SurveyRequest): SurveyResponse
}
