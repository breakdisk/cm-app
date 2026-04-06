package io.logisticos.driver.core.network.service

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import retrofit2.Response
import retrofit2.http.Body
import retrofit2.http.POST

@Serializable
data class BreadcrumbPoint(
    val lat: Double,
    val lng: Double,
    val accuracy: Float,
    @SerialName("speed_mps") val speedMps: Float,
    val bearing: Float,
    val timestamp: Long
)

@Serializable
data class BreadcrumbBatchRequest(
    @SerialName("shift_id") val shiftId: String,
    val points: List<BreadcrumbPoint>
)

interface TrackingApiService {
    @POST("location/batch")
    suspend fun uploadBreadcrumbs(@Body request: BreadcrumbBatchRequest): Response<Unit>
}
